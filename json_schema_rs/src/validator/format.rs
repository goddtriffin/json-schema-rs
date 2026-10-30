//! Checkers for the `format` keyword's string formats.
//!
//! Each checker takes the instance string and answers whether it is a member of the format's
//! syntax. Formats apply only to strings; callers skip every other instance type. Checkers are
//! hand-written, allocation-free scanners wherever the grammar allows (`regex` compiles the
//! pattern; `uri`, `uri-reference`, `iri` and `iri-reference` use `fluent-uri`'s RFC 3986 /
//! RFC 3987 parsers, which borrow the input).

mod idna_table;

use super::ecma_regex;

use idna_table::{
    GREEK, HEBREW, IDNA_VALID, JOIN_DUAL, JOIN_LEFT, JOIN_RIGHT, JOIN_TRANSPARENT, KANA_HAN, MARK,
    VIRAMA,
};

/// A `format` this library checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Format {
    /// RFC 3339 `date-time`.
    DateTime,
    /// RFC 3339 `full-date`.
    Date,
    /// RFC 3339 `full-time`.
    Time,
    /// RFC 3339 Appendix A `duration`.
    Duration,
    /// RFC 5321 `Mailbox`.
    Email,
    /// RFC 6531 `Mailbox`.
    IdnEmail,
    /// RFC 1123 host name; `xn--` labels must be IDNA2008 A-labels.
    Hostname,
    /// IDNA2008 (RFC 5890 - 5892) internationalised host name.
    IdnHostname,
    /// RFC 2673 dotted-quad IPv4 address.
    Ipv4,
    /// RFC 4291 IPv6 address.
    Ipv6,
    /// RFC 3986 URI.
    Uri,
    /// RFC 3986 URI reference.
    UriReference,
    /// RFC 3987 IRI.
    Iri,
    /// RFC 3987 IRI reference.
    IriReference,
    /// RFC 6570 URI template.
    UriTemplate,
    /// RFC 6901 JSON Pointer.
    JsonPointer,
    /// Relative JSON Pointer.
    RelativeJsonPointer,
    /// ECMA-262 regular expression.
    Regex,
    /// RFC 4122 UUID.
    Uuid,
}

impl Format {
    /// The format named `name`, or `None` for a format this library does not check.
    pub(crate) fn from_name(name: &str) -> Option<Format> {
        let format: Format = match name {
            "date-time" => Format::DateTime,
            "date" => Format::Date,
            "time" => Format::Time,
            "duration" => Format::Duration,
            "email" => Format::Email,
            "idn-email" => Format::IdnEmail,
            "hostname" => Format::Hostname,
            "idn-hostname" => Format::IdnHostname,
            "ipv4" => Format::Ipv4,
            "ipv6" => Format::Ipv6,
            "uri" => Format::Uri,
            "uri-reference" => Format::UriReference,
            "iri" => Format::Iri,
            "iri-reference" => Format::IriReference,
            "uri-template" => Format::UriTemplate,
            "json-pointer" => Format::JsonPointer,
            "relative-json-pointer" => Format::RelativeJsonPointer,
            "regex" => Format::Regex,
            "uuid" => Format::Uuid,
            _ => return None,
        };
        Some(format)
    }

    /// The format's name as written in a schema.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Format::DateTime => "date-time",
            Format::Date => "date",
            Format::Time => "time",
            Format::Duration => "duration",
            Format::Email => "email",
            Format::IdnEmail => "idn-email",
            Format::Hostname => "hostname",
            Format::IdnHostname => "idn-hostname",
            Format::Ipv4 => "ipv4",
            Format::Ipv6 => "ipv6",
            Format::Uri => "uri",
            Format::UriReference => "uri-reference",
            Format::Iri => "iri",
            Format::IriReference => "iri-reference",
            Format::UriTemplate => "uri-template",
            Format::JsonPointer => "json-pointer",
            Format::RelativeJsonPointer => "relative-json-pointer",
            Format::Regex => "regex",
            Format::Uuid => "uuid",
        }
    }

    /// Whether `value` is a member of this format.
    pub(crate) fn is_valid(self, value: &str) -> bool {
        match self {
            Format::DateTime => is_valid_date_time(value.as_bytes()),
            Format::Date => is_valid_date(value.as_bytes()),
            Format::Time => is_valid_time(value.as_bytes()),
            Format::Duration => is_valid_duration(value.as_bytes()),
            Format::Email => is_valid_email(value, false),
            Format::IdnEmail => is_valid_email(value, true),
            Format::Hostname => is_valid_hostname(value),
            Format::IdnHostname => is_valid_idn_hostname(value),
            Format::Ipv4 => is_valid_ipv4(value.as_bytes()),
            Format::Ipv6 => is_valid_ipv6(value.as_bytes()),
            Format::Uri => fluent_uri::Uri::parse(value).is_ok(),
            Format::UriReference => fluent_uri::UriRef::parse(value).is_ok(),
            Format::Iri => fluent_uri::Iri::parse(value).is_ok(),
            Format::IriReference => fluent_uri::IriRef::parse(value).is_ok(),
            Format::UriTemplate => is_valid_uri_template(value),
            Format::JsonPointer => is_valid_json_pointer(value.as_bytes()),
            Format::RelativeJsonPointer => is_valid_relative_json_pointer(value.as_bytes()),
            Format::Regex => ecma_regex::compile(value).is_ok(),
            Format::Uuid => is_valid_uuid(value.as_bytes()),
        }
    }
}

// --- Dates and times (RFC 3339) -----------------------------------------------------------------

/// The two ASCII digits at `bytes[at..at + 2]` as a number.
fn two_digits(bytes: &[u8], at: usize) -> Option<u32> {
    let tens: u8 = *bytes.get(at)?;
    let ones: u8 = *bytes.get(at + 1)?;
    if tens.is_ascii_digit() && ones.is_ascii_digit() {
        Some(u32::from(tens - b'0') * 10 + u32::from(ones - b'0'))
    } else {
        None
    }
}

/// RFC 3339 `full-date`: `YYYY-MM-DD` naming a day that exists (leap years included).
fn is_valid_date(bytes: &[u8]) -> bool {
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let (Some(century), Some(year_of_century), Some(month), Some(day)) = (
        two_digits(bytes, 0),
        two_digits(bytes, 2),
        two_digits(bytes, 5),
        two_digits(bytes, 8),
    ) else {
        return false;
    };
    let year: u32 = century * 100 + year_of_century;
    let leap: bool =
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days_in_month: u32 = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}

/// RFC 3339 `full-time`: `HH:MM:SS[.frac]` followed by `Z` or `±HH:MM` (both required, `Z`
/// case-insensitive). Second 60 is a leap second, valid only when the time in UTC is 23:59:60.
fn is_valid_time(bytes: &[u8]) -> bool {
    if bytes.get(2) != Some(&b':') || bytes.get(5) != Some(&b':') {
        return false;
    }
    let (Some(hour), Some(minute), Some(second)) = (
        two_digits(bytes, 0),
        two_digits(bytes, 3),
        two_digits(bytes, 6),
    ) else {
        return false;
    };
    let mut at: usize = 8;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let digits_start: usize = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == digits_start {
            return false;
        }
    }
    // Offset in minutes east of UTC.
    let offset: i32 = match bytes.get(at) {
        Some(b'Z' | b'z') if bytes.len() == at + 1 => 0,
        Some(&sign @ (b'+' | b'-')) if bytes.len() == at + 6 && bytes[at + 3] == b':' => {
            let (Some(offset_hour), Some(offset_minute)) =
                (two_digits(bytes, at + 1), two_digits(bytes, at + 4))
            else {
                return false;
            };
            if offset_hour > 23 || offset_minute > 59 {
                return false;
            }
            let magnitude: i32 = i32::try_from(offset_hour * 60 + offset_minute).unwrap_or(0);
            if sign == b'+' { magnitude } else { -magnitude }
        }
        _ => return false,
    };
    if hour > 23 || minute > 59 || second > 60 {
        return false;
    }
    if second == 60 {
        let local: i32 = i32::try_from(hour * 60 + minute).unwrap_or(0);
        return (local - offset).rem_euclid(24 * 60) == 23 * 60 + 59;
    }
    true
}

/// RFC 3339 `date-time`: `full-date` `T` `full-time`, `T` case-insensitive.
fn is_valid_date_time(bytes: &[u8]) -> bool {
    bytes.len() > 11
        && matches!(bytes[10], b'T' | b't')
        && is_valid_date(&bytes[..10])
        && is_valid_time(&bytes[11..])
}

/// RFC 3339 Appendix A `duration`: `P` then a date part (`Y [M [D]]` as a contiguous run, e.g.
/// `P1Y2M`, `P2M3D`), an optional time part `T` (`H [M [S]]` likewise), or a lone week count
/// `PnW`. At least one element is required, and a `T` must be followed by one.
fn is_valid_duration(bytes: &[u8]) -> bool {
    let Some(rest) = bytes.strip_prefix(b"P") else {
        return false;
    };
    let (date, time): (&[u8], Option<&[u8]>) = match rest.iter().position(|&byte| byte == b'T') {
        Some(separator) => (&rest[..separator], Some(&rest[separator + 1..])),
        None => (rest, None),
    };
    if let Some(weeks) = date.strip_suffix(b"W") {
        return time.is_none() && !weeks.is_empty() && weeks.iter().all(u8::is_ascii_digit);
    }
    if !duration_elements_in_order(date, *b"YMD") {
        return false;
    }
    match time {
        None => !date.is_empty(),
        Some(time) => !time.is_empty() && duration_elements_in_order(time, *b"HMS"),
    }
}

/// Whether `bytes` is a sequence of `1*DIGIT unit` elements whose units form a contiguous run of
/// `units` in order (`Y`, `YM`, `YMD`, `M`, `MD` or `D` for the date part). Empty input is a run
/// of no elements.
fn duration_elements_in_order(bytes: &[u8], units: [u8; 3]) -> bool {
    let mut at: usize = 0;
    let mut expected: Option<usize> = None;
    while at < bytes.len() {
        let digits_start: usize = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        if at == digits_start || at == bytes.len() {
            return false;
        }
        let Some(unit) = units.iter().position(|&unit| unit == bytes[at]) else {
            return false;
        };
        if expected.is_some_and(|expected| expected != unit) {
            return false;
        }
        expected = Some(unit + 1);
        at += 1;
    }
    true
}

// --- Email (RFC 5321 / RFC 6531) ----------------------------------------------------------------

/// RFC 5322 `atext`.
fn is_atext(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&byte)
}

/// RFC 5321 `Mailbox`: a dot-atom or quoted-string local part, `@`, and a domain that is a host
/// name or an address literal (`[192.0.2.1]`, `[IPv6:2001:db8::1]`). With `international`
/// (RFC 6531), the local part also admits non-ASCII characters and the domain is an
/// `idn-hostname`.
fn is_valid_email(value: &str, international: bool) -> bool {
    let bytes: &[u8] = value.as_bytes();
    let at_sign: usize = if bytes.first() == Some(&b'"') {
        let mut at: usize = 1;
        loop {
            match bytes.get(at) {
                None => return false,
                Some(b'"') => break,
                Some(b'\\') => {
                    let escaped_ok: bool = bytes.get(at + 1).is_some_and(|&byte| {
                        (0x20..=0x7E).contains(&byte) || (international && byte >= 0x80)
                    });
                    if !escaped_ok {
                        return false;
                    }
                    at += 2;
                }
                Some(&byte) => {
                    let qtext: bool = matches!(byte, 0x20..=0x21 | 0x23..=0x5B | 0x5D..=0x7E)
                        || (international && byte >= 0x80);
                    if !qtext {
                        return false;
                    }
                    at += 1;
                }
            }
        }
        at + 1
    } else {
        let Some(at_sign) = bytes.iter().position(|&byte| byte == b'@') else {
            return false;
        };
        let local: &[u8] = &bytes[..at_sign];
        let dot_atom: bool = !local.is_empty()
            && local[0] != b'.'
            && local[local.len() - 1] != b'.'
            && !local.windows(2).any(|pair| pair == b"..")
            && local
                .iter()
                .all(|&byte| byte == b'.' || is_atext(byte) || (international && byte >= 0x80));
        if !dot_atom {
            return false;
        }
        at_sign
    };
    if bytes.get(at_sign) != Some(&b'@') {
        return false;
    }
    let domain: &str = &value[at_sign + 1..];
    if let Some(literal) = domain
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        let literal: &[u8] = literal.as_bytes();
        return match literal.get(..5) {
            Some(tag) if tag.eq_ignore_ascii_case(b"IPv6:") => is_valid_ipv6(&literal[5..]),
            _ => is_valid_ipv4(literal),
        };
    }
    if international {
        is_valid_idn_hostname(domain)
    } else {
        is_valid_hostname(domain)
    }
}

// --- Host names (RFC 1123, IDNA2008) ------------------------------------------------------------

/// Longest host name in its ASCII form.
const MAX_HOSTNAME_LEN: usize = 253;
/// Longest label in its ASCII form.
const MAX_LABEL_LEN: usize = 63;

/// RFC 1123 host name: dot-separated labels of 1 - 63 letters, digits and hyphens, neither
/// starting nor ending with a hyphen, at most 253 characters in all. A label starting `xn--`
/// (any case) must be an IDNA2008 A-label.
fn is_valid_hostname(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HOSTNAME_LEN
        && value
            .split('.')
            .all(|label| is_valid_ascii_label(label.as_bytes()))
}

/// One ASCII host name label (see [`is_valid_hostname`]).
fn is_valid_ascii_label(label: &[u8]) -> bool {
    let shape_ok: bool = !label.is_empty()
        && label.len() <= MAX_LABEL_LEN
        && label[0] != b'-'
        && label[label.len() - 1] != b'-'
        && label
            .iter()
            .all(|&byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if !shape_ok {
        return false;
    }
    match label.get(..4) {
        Some(prefix) if prefix.eq_ignore_ascii_case(b"xn--") => is_valid_a_label(&label[4..]),
        _ => true,
    }
}

/// IDNA2008 internationalised host name: labels separated by `.` or its ideographic / fullwidth
/// forms (U+3002, U+FF0E, U+FF61), each an ASCII label (see [`is_valid_hostname`]) or a valid
/// U-label, with the ASCII (A-label) form of the whole name at most 253 characters.
fn is_valid_idn_hostname(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let mut ascii_len: usize = 0;
    for label in value.split(['.', '\u{3002}', '\u{FF0E}', '\u{FF61}']) {
        let label_len: Option<usize> = if label.is_ascii() {
            is_valid_ascii_label(label.as_bytes()).then_some(label.len())
        } else {
            u_label_ascii_len(label)
        };
        let Some(label_len) = label_len else {
            return false;
        };
        ascii_len += label_len + usize::from(ascii_len != 0);
    }
    ascii_len <= MAX_HOSTNAME_LEN
}

/// The length of a non-ASCII label's A-label form, or `None` when it is not a valid U-label or
/// its A-label exceeds 63 characters.
fn u_label_ascii_len(label: &str) -> Option<usize> {
    // Every code point costs at least one A-label character, so a label of more than 63 code
    // points can never fit.
    let mut buffer: [char; MAX_LABEL_LEN] = ['\0'; MAX_LABEL_LEN];
    let mut len: usize = 0;
    for character in label.chars() {
        *buffer.get_mut(len)? = character;
        len += 1;
    }
    let characters: &[char] = &buffer[..len];
    if !is_valid_u_label(characters) {
        return None;
    }
    let ascii_len: usize = "xn--".len() + punycode_encoded_len(characters)?;
    (ascii_len <= MAX_LABEL_LEN).then_some(ascii_len)
}

/// Whether the Punycode after an `xn--` prefix decodes to a valid U-label containing at least
/// one non-ASCII code point.
fn is_valid_a_label(encoded: &[u8]) -> bool {
    let mut buffer: [char; MAX_LABEL_LEN] = ['\0'; MAX_LABEL_LEN];
    let Some(len) = punycode_decode(encoded, &mut buffer) else {
        return false;
    };
    let characters: &[char] = &buffer[..len];
    characters.iter().any(|character| !character.is_ascii()) && is_valid_u_label(characters)
}

/// The IDNA2008 table flags of a non-ASCII code point, or `None` when it is `DISALLOWED` or
/// `UNASSIGNED`.
fn idna_flags(character: char) -> Option<u16> {
    let code_point: u32 = u32::from(character);
    let index: usize = IDNA_VALID.partition_point(|&(first, _, _)| first <= code_point);
    let &(_, last, flags) = IDNA_VALID.get(index.checked_sub(1)?)?;
    (code_point <= last).then_some(flags)
}

/// Whether `character` is non-ASCII and has all of `flags`.
fn has_flags(character: char, flags: u16) -> bool {
    !character.is_ascii() && idna_flags(character).is_some_and(|found| found & flags != 0)
}

/// RFC 5891 section 4.2.3 / RFC 5892 U-label: every code point `PVALID` (ASCII letters in either
/// case, digits, hyphen, or a table entry) or `CONTEXTJ` / `CONTEXTO` with its Appendix A rule
/// satisfied; no hyphen first, last, or in both the third and fourth positions; no combining
/// mark first.
fn is_valid_u_label(label: &[char]) -> bool {
    let (Some(&first), Some(&last)) = (label.first(), label.last()) else {
        return false;
    };
    if first == '-' || last == '-' || (label.len() >= 4 && label[2] == '-' && label[3] == '-') {
        return false;
    }
    if has_flags(first, MARK) {
        return false;
    }
    let mut has_katakana_middle_dot: bool = false;
    let mut has_kana_han: bool = false;
    let mut has_arabic_indic_digit: bool = false;
    let mut has_extended_arabic_indic_digit: bool = false;
    for (index, &character) in label.iter().enumerate() {
        if character.is_ascii() {
            if !(character.is_ascii_alphanumeric() || character == '-') {
                return false;
            }
            continue;
        }
        let Some(flags) = idna_flags(character) else {
            return false;
        };
        let previous: Option<char> = index.checked_sub(1).map(|previous| label[previous]);
        let next: Option<char> = label.get(index + 1).copied();
        let previous_is_virama: bool = previous.is_some_and(|previous| has_flags(previous, VIRAMA));
        let context_ok: bool = match character {
            // ZERO WIDTH NON-JOINER
            '\u{200C}' => previous_is_virama || zero_width_non_joiner_joins(label, index),
            // ZERO WIDTH JOINER
            '\u{200D}' => previous_is_virama,
            // MIDDLE DOT
            '\u{00B7}' => previous == Some('l') && next == Some('l'),
            // GREEK LOWER NUMERAL SIGN (KERAIA)
            '\u{0375}' => next.is_some_and(|next| has_flags(next, GREEK)),
            // HEBREW PUNCTUATION GERESH / GERSHAYIM
            '\u{05F3}' | '\u{05F4}' => previous.is_some_and(|previous| has_flags(previous, HEBREW)),
            // KATAKANA MIDDLE DOT
            '\u{30FB}' => {
                has_katakana_middle_dot = true;
                true
            }
            '\u{0660}'..='\u{0669}' => {
                has_arabic_indic_digit = true;
                true
            }
            '\u{06F0}'..='\u{06F9}' => {
                has_extended_arabic_indic_digit = true;
                true
            }
            _ => true,
        };
        if !context_ok {
            return false;
        }
        has_kana_han |= flags & KANA_HAN != 0;
    }
    (!has_katakana_middle_dot || has_kana_han)
        && !(has_arabic_indic_digit && has_extended_arabic_indic_digit)
}

/// RFC 5892 Appendix A.1 regular expression for the ZERO WIDTH NON-JOINER at `index`:
/// `(Joining_Type:{L,D})(Joining_Type:T)* ZWNJ (Joining_Type:T)*(Joining_Type:{R,D})`.
fn zero_width_non_joiner_joins(label: &[char], index: usize) -> bool {
    let before: Option<&char> = label[..index]
        .iter()
        .rev()
        .find(|&&character| !has_flags(character, JOIN_TRANSPARENT));
    let after: Option<&char> = label[index + 1..]
        .iter()
        .find(|&&character| !has_flags(character, JOIN_TRANSPARENT));
    before.is_some_and(|&before| has_flags(before, JOIN_LEFT | JOIN_DUAL))
        && after.is_some_and(|&after| has_flags(after, JOIN_RIGHT | JOIN_DUAL))
}

// --- Punycode (RFC 3492) ------------------------------------------------------------------------

const PUNYCODE_BASE: u32 = 36;
const PUNYCODE_T_MIN: u32 = 1;
const PUNYCODE_T_MAX: u32 = 26;
const PUNYCODE_SKEW: u32 = 38;
const PUNYCODE_DAMP: u32 = 700;
const PUNYCODE_INITIAL_BIAS: u32 = 72;
const PUNYCODE_INITIAL_N: u32 = 0x80;

/// RFC 3492 section 6.1 bias adaptation.
fn punycode_adapt(delta: u32, points: u32, first_time: bool) -> u32 {
    let mut delta: u32 = if first_time {
        delta / PUNYCODE_DAMP
    } else {
        delta / 2
    };
    delta += delta / points;
    let mut k: u32 = 0;
    while delta > ((PUNYCODE_BASE - PUNYCODE_T_MIN) * PUNYCODE_T_MAX) / 2 {
        delta /= PUNYCODE_BASE - PUNYCODE_T_MIN;
        k += PUNYCODE_BASE;
    }
    k + (PUNYCODE_BASE - PUNYCODE_T_MIN + 1) * delta / (delta + PUNYCODE_SKEW)
}

/// The threshold `t` for the digit at position `k` (RFC 3492 section 6.2).
fn punycode_threshold(k: u32, bias: u32) -> u32 {
    k.saturating_sub(bias).clamp(PUNYCODE_T_MIN, PUNYCODE_T_MAX)
}

/// RFC 3492 section 6.2 decoding into `output`; `None` on malformed input, overflow, a basic
/// code point encoded as a delta, or more code points than `output` holds.
fn punycode_decode(input: &[u8], output: &mut [char]) -> Option<usize> {
    let (basic, extended): (&[u8], &[u8]) = match input.iter().rposition(|&byte| byte == b'-') {
        Some(delimiter) => (&input[..delimiter], &input[delimiter + 1..]),
        None => (&[], input),
    };
    let mut len: usize = 0;
    for &byte in basic {
        if !byte.is_ascii() {
            return None;
        }
        *output.get_mut(len)? = char::from(byte);
        len += 1;
    }
    let mut n: u32 = PUNYCODE_INITIAL_N;
    let mut i: u32 = 0;
    let mut bias: u32 = PUNYCODE_INITIAL_BIAS;
    let mut at: usize = 0;
    while at < extended.len() {
        let old_i: u32 = i;
        let mut weight: u32 = 1;
        let mut k: u32 = PUNYCODE_BASE;
        loop {
            let byte: u8 = *extended.get(at)?;
            at += 1;
            let digit: u32 = match byte {
                b'a'..=b'z' => u32::from(byte - b'a'),
                b'A'..=b'Z' => u32::from(byte - b'A'),
                b'0'..=b'9' => u32::from(byte - b'0') + 26,
                _ => return None,
            };
            i = i.checked_add(digit.checked_mul(weight)?)?;
            let threshold: u32 = punycode_threshold(k, bias);
            if digit < threshold {
                break;
            }
            weight = weight.checked_mul(PUNYCODE_BASE - threshold)?;
            k += PUNYCODE_BASE;
        }
        let points: u32 = u32::try_from(len + 1).ok()?;
        bias = punycode_adapt(i - old_i, points, old_i == 0);
        n = n.checked_add(i / points)?;
        i %= points;
        if n < PUNYCODE_INITIAL_N {
            return None;
        }
        let character: char = char::from_u32(n)?;
        if len == output.len() {
            return None;
        }
        let position: usize = usize::try_from(i).ok()?;
        output.copy_within(position..len, position + 1);
        output[position] = character;
        len += 1;
        i += 1;
    }
    Some(len)
}

/// The length of the RFC 3492 section 6.3 encoding of `input` (without the `xn--` prefix), or
/// `None` on overflow.
fn punycode_encoded_len(input: &[char]) -> Option<usize> {
    let basic: usize = input
        .iter()
        .filter(|character| character.is_ascii())
        .count();
    let mut len: usize = basic + usize::from(basic > 0);
    let total: u32 = u32::try_from(input.len()).ok()?;
    let mut handled: u32 = u32::try_from(basic).ok()?;
    let basic_count: u32 = handled;
    let mut n: u32 = PUNYCODE_INITIAL_N;
    let mut delta: u32 = 0;
    let mut bias: u32 = PUNYCODE_INITIAL_BIAS;
    while handled < total {
        let next: u32 = input
            .iter()
            .map(|&character| u32::from(character))
            .filter(|&code_point| code_point >= n)
            .min()?;
        delta = delta.checked_add((next - n).checked_mul(handled + 1)?)?;
        n = next;
        for &character in input {
            let code_point: u32 = u32::from(character);
            if code_point < n {
                delta = delta.checked_add(1)?;
            } else if code_point == n {
                let mut q: u32 = delta;
                let mut k: u32 = PUNYCODE_BASE;
                loop {
                    let threshold: u32 = punycode_threshold(k, bias);
                    if q < threshold {
                        break;
                    }
                    len += 1;
                    q = (q - threshold) / (PUNYCODE_BASE - threshold);
                    k += PUNYCODE_BASE;
                }
                len += 1;
                bias = punycode_adapt(delta, handled + 1, handled == basic_count);
                delta = 0;
                handled += 1;
            }
        }
        delta = delta.checked_add(1)?;
        n += 1;
    }
    Some(len)
}

// --- IP addresses (RFC 2673, RFC 4291) ----------------------------------------------------------

/// Dotted-quad IPv4: four decimal octets 0 - 255 without leading zeros.
fn is_valid_ipv4(bytes: &[u8]) -> bool {
    let mut octets: usize = 0;
    for octet in bytes.split(|&byte| byte == b'.') {
        octets += 1;
        let shape_ok: bool = octets <= 4
            && (1..=3).contains(&octet.len())
            && octet.iter().all(u8::is_ascii_digit)
            && (octet.len() == 1 || octet[0] != b'0');
        if !shape_ok {
            return false;
        }
        let value: u32 = octet
            .iter()
            .fold(0, |value, &digit| value * 10 + u32::from(digit - b'0'));
        if value > 255 {
            return false;
        }
    }
    octets == 4
}

/// RFC 4291 section 2.2 IPv6 text form: eight 1 - 4 digit hex groups, at most one `::` standing
/// for one or more zero groups, and optionally a dotted-quad IPv4 tail in place of the last two
/// groups. Zone identifiers and prefix lengths are not part of an address.
fn is_valid_ipv6(bytes: &[u8]) -> bool {
    let compression: Option<usize> = bytes.windows(2).position(|pair| pair == b"::");
    match compression {
        Some(at) => {
            let head: &[u8] = &bytes[..at];
            let tail: &[u8] = &bytes[at + 2..];
            match (ipv6_group_count(head, false), ipv6_group_count(tail, true)) {
                (Some(head_groups), Some(tail_groups)) => head_groups + tail_groups <= 7,
                _ => false,
            }
        }
        None => ipv6_group_count(bytes, true) == Some(8),
    }
}

/// The number of 16-bit groups in a `:`-separated run of hex groups (an IPv4 tail counts two),
/// or `None` when malformed. An empty run has no groups.
fn ipv6_group_count(bytes: &[u8], ipv4_tail_allowed: bool) -> Option<usize> {
    if bytes.is_empty() {
        return Some(0);
    }
    let mut groups: usize = 0;
    let mut segments = bytes.split(|&byte| byte == b':').peekable();
    while let Some(segment) = segments.next() {
        let is_last: bool = segments.peek().is_none();
        if is_last && ipv4_tail_allowed && segment.contains(&b'.') {
            if !is_valid_ipv4(segment) {
                return None;
            }
            groups += 2;
        } else {
            if !(1..=4).contains(&segment.len()) || !segment.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            groups += 1;
        }
    }
    Some(groups)
}

// --- URI templates (RFC 6570) -------------------------------------------------------------------

/// RFC 3987 `ucschar` or `iprivate`.
fn is_ucschar_or_iprivate(character: char) -> bool {
    let code_point: u32 = u32::from(character);
    let plane_offset: u32 = code_point & 0xFFFF;
    match code_point {
        0xA0..=0xD7FF | 0xE000..=0xFDCF | 0xFDF0..=0xFFEF => true,
        0x1_0000..=0x10_FFFF => plane_offset <= 0xFFFD,
        _ => false,
    }
}

/// RFC 6570 level 4 URI template: literals (URI characters other than space, `"`, `'`, `<`,
/// `>`, `\`, `^`, the backtick, `{`, `|` and `}`, plus
/// `ucschar` / `iprivate` and percent-encodings) and `{` expressions `}` of an optional operator
/// and comma-separated variables, each a dotted name of `ALPHA` / `DIGIT` / `_` / percent-encoded
/// characters with an optional `:1-9999` prefix or `*` explode modifier.
fn is_valid_uri_template(value: &str) -> bool {
    let bytes: &[u8] = value.as_bytes();
    let mut at: usize = 0;
    while at < bytes.len() {
        let byte: u8 = bytes[at];
        let next: Option<usize> = match byte {
            b'{' => uri_template_expression_end(bytes, at + 1),
            b'%' => percent_encoded_end(bytes, at),
            0x21
            | 0x23..=0x24
            | 0x26
            | 0x28..=0x3B
            | 0x3D
            | 0x3F..=0x5B
            | 0x5D
            | 0x5F
            | 0x61..=0x7A
            | 0x7E => Some(at + 1),
            0x80.. => value[at..]
                .chars()
                .next()
                .filter(|&character| is_ucschar_or_iprivate(character))
                .map(|character| at + character.len_utf8()),
            _ => None,
        };
        let Some(next) = next else {
            return false;
        };
        at = next;
    }
    true
}

/// The index after the percent-encoding `%XX` at `at`.
fn percent_encoded_end(bytes: &[u8], at: usize) -> Option<usize> {
    let hex_ok: bool = bytes
        .get(at + 1..at + 3)
        .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit));
    hex_ok.then_some(at + 3)
}

/// The index after the `varchar` at `at`.
fn uri_template_varchar_end(bytes: &[u8], at: usize) -> Option<usize> {
    match *bytes.get(at)? {
        b'%' => percent_encoded_end(bytes, at),
        byte if byte.is_ascii_alphanumeric() || byte == b'_' => Some(at + 1),
        _ => None,
    }
}

/// The index after the `}` closing the expression whose body starts at `at`.
fn uri_template_expression_end(bytes: &[u8], mut at: usize) -> Option<usize> {
    if bytes
        .get(at)
        .is_some_and(|byte| b"+#./;?&=,!@|".contains(byte))
    {
        at += 1;
    }
    loop {
        at = uri_template_varchar_end(bytes, at)?;
        loop {
            if bytes.get(at) == Some(&b'.') {
                at = uri_template_varchar_end(bytes, at + 1)?;
            } else if let Some(end) = uri_template_varchar_end(bytes, at) {
                at = end;
            } else {
                break;
            }
        }
        match bytes.get(at) {
            Some(b':') => {
                at += 1;
                if !bytes
                    .get(at)
                    .is_some_and(|byte| (b'1'..=b'9').contains(byte))
                {
                    return None;
                }
                at += 1;
                let mut extra_digits: usize = 0;
                while extra_digits < 3 && bytes.get(at).is_some_and(u8::is_ascii_digit) {
                    at += 1;
                    extra_digits += 1;
                }
            }
            Some(b'*') => at += 1,
            _ => {}
        }
        match bytes.get(at)? {
            b',' => at += 1,
            b'}' => return Some(at + 1),
            _ => return None,
        }
    }
}

// --- JSON Pointers (RFC 6901, Relative JSON Pointer) --------------------------------------------

/// RFC 6901 JSON Pointer: empty, or `/`-prefixed reference tokens where `~` is always followed
/// by `0` or `1`.
fn is_valid_json_pointer(bytes: &[u8]) -> bool {
    if bytes.first().is_some_and(|&byte| byte != b'/') {
        return false;
    }
    let mut at: usize = 0;
    while at < bytes.len() {
        if bytes[at] == b'~' {
            if !matches!(bytes.get(at + 1), Some(b'0' | b'1')) {
                return false;
            }
            at += 2;
        } else {
            at += 1;
        }
    }
    true
}

/// Relative JSON Pointer: a non-negative integer without leading zeros, an optional index
/// manipulation (`+` or `-` and a positive integer), then `#` or a JSON Pointer.
fn is_valid_relative_json_pointer(bytes: &[u8]) -> bool {
    let digits: usize = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 || (digits > 1 && bytes[0] == b'0') {
        return false;
    }
    let mut rest: &[u8] = &bytes[digits..];
    if let Some(after_sign) = rest.strip_prefix(b"+").or_else(|| rest.strip_prefix(b"-")) {
        let index_digits: usize = after_sign
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if index_digits == 0 || after_sign[0] == b'0' {
            return false;
        }
        rest = &after_sign[index_digits..];
    }
    rest == b"#" || is_valid_json_pointer(rest)
}

// --- UUID (RFC 4122) ----------------------------------------------------------------------------

/// RFC 4122 `8-4-4-4-12` hex digits with hyphens, any version and variant, either case.
fn is_valid_uuid(bytes: &[u8]) -> bool {
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, &byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::{Format, u_label_ascii_len};

    const ALL_FORMATS: [Format; 19] = [
        Format::DateTime,
        Format::Date,
        Format::Time,
        Format::Duration,
        Format::Email,
        Format::IdnEmail,
        Format::Hostname,
        Format::IdnHostname,
        Format::Ipv4,
        Format::Ipv6,
        Format::Uri,
        Format::UriReference,
        Format::Iri,
        Format::IriReference,
        Format::UriTemplate,
        Format::JsonPointer,
        Format::RelativeJsonPointer,
        Format::Regex,
        Format::Uuid,
    ];

    #[test]
    fn from_name_round_trips_every_format_name() {
        let expected: Vec<Option<Format>> = ALL_FORMATS.iter().copied().map(Some).collect();
        let actual: Vec<Option<Format>> = ALL_FORMATS
            .iter()
            .map(|format| Format::from_name(format.name()))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn name_returns_schema_spelling() {
        let expected: Vec<&str> = vec![
            "date-time",
            "date",
            "time",
            "duration",
            "email",
            "idn-email",
            "hostname",
            "idn-hostname",
            "ipv4",
            "ipv6",
            "uri",
            "uri-reference",
            "iri",
            "iri-reference",
            "uri-template",
            "json-pointer",
            "relative-json-pointer",
            "regex",
            "uuid",
        ];
        let actual: Vec<&str> = ALL_FORMATS.iter().map(|format| format.name()).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_name_returns_none_for_unknown_format() {
        let expected: Option<Format> = None;
        let actual: Option<Format> = Format::from_name("color");
        assert_eq!(expected, actual);
    }

    #[test]
    fn u_label_ascii_len_counts_xn_prefix_and_punycode() {
        // "실례" is "xn--9n2bp8q".
        let expected: Option<usize> = Some(11);
        let actual: Option<usize> = u_label_ascii_len("실례");
        assert_eq!(expected, actual);
    }

    #[test]
    fn u_label_ascii_len_counts_basic_code_points_and_delimiter() {
        // "ll·" with a middle dot between the l's is "xn--ll-0ea".
        let expected: Option<usize> = Some(10);
        let actual: Option<usize> = u_label_ascii_len("l\u{b7}l");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_accepts_fractional_seconds_and_offset() {
        let expected: bool = true;
        let actual: bool = Format::DateTime.is_valid("1937-01-01T12:00:27.87+00:20");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_accepts_lowercase_t_and_z() {
        let expected: bool = true;
        let actual: bool = Format::DateTime.is_valid("1963-06-19t08:30:06.283185z");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_accepts_leap_second_at_utc_midnight() {
        let expected: bool = true;
        let actual: bool = Format::DateTime.is_valid("1998-12-31T23:59:60Z");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_accepts_leap_second_with_negative_offset() {
        let expected: bool = true;
        let actual: bool = Format::DateTime.is_valid("1998-12-31T15:59:60.123-08:00");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_rejects_leap_second_on_wrong_minute() {
        let expected: bool = false;
        let actual: bool = Format::DateTime.is_valid("1998-12-31T23:58:60Z");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_rejects_nonexistent_day() {
        let expected: bool = false;
        let actual: bool = Format::DateTime.is_valid("1990-02-31T15:59:59.123-08:00");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_rejects_missing_offset() {
        let expected: bool = false;
        let actual: bool = Format::DateTime.is_valid("1963-06-19T08:30:06");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_rejects_offset_followed_by_z() {
        let expected: bool = false;
        let actual: bool = Format::DateTime.is_valid("1963-06-19T08:30:06.28123+01:00Z");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_time_rejects_space_separator() {
        let expected: bool = false;
        let actual: bool = Format::DateTime.is_valid("1963-06-19 08:30:06Z");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_accepts_february_29_in_leap_year() {
        let expected: bool = true;
        let actual: bool = Format::Date.is_valid("2020-02-29");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_accepts_february_29_in_400_year() {
        let expected: bool = true;
        let actual: bool = Format::Date.is_valid("2000-02-29");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_rejects_february_29_in_century_year() {
        let expected: bool = false;
        let actual: bool = Format::Date.is_valid("1900-02-29");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_rejects_day_31_in_30_day_month() {
        let expected: bool = false;
        let actual: bool = Format::Date.is_valid("2020-04-31");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_rejects_month_13() {
        let expected: bool = false;
        let actual: bool = Format::Date.is_valid("2020-13-01");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_rejects_unpadded_month() {
        let expected: bool = false;
        let actual: bool = Format::Date.is_valid("1998-1-20");
        assert_eq!(expected, actual);
    }

    #[test]
    fn date_rejects_non_ascii_digit() {
        let expected: bool = false;
        let actual: bool = Format::Date.is_valid("1963-06-1\u{9ea}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn time_accepts_utc_time() {
        let expected: bool = true;
        let actual: bool = Format::Time.is_valid("08:30:06Z");
        assert_eq!(expected, actual);
    }

    #[test]
    fn time_accepts_leap_second_with_positive_offset() {
        let expected: bool = true;
        let actual: bool = Format::Time.is_valid("01:29:60+01:30");
        assert_eq!(expected, actual);
    }

    #[test]
    fn time_rejects_leap_second_not_at_utc_midnight() {
        let expected: bool = false;
        let actual: bool = Format::Time.is_valid("23:59:60+01:00");
        assert_eq!(expected, actual);
    }

    #[test]
    fn time_rejects_missing_offset() {
        let expected: bool = false;
        let actual: bool = Format::Time.is_valid("12:00:00");
        assert_eq!(expected, actual);
    }

    #[test]
    fn time_rejects_hour_24() {
        let expected: bool = false;
        let actual: bool = Format::Time.is_valid("24:00:00Z");
        assert_eq!(expected, actual);
    }

    #[test]
    fn time_rejects_offset_hour_24() {
        let expected: bool = false;
        let actual: bool = Format::Time.is_valid("01:02:03+24:00");
        assert_eq!(expected, actual);
    }

    #[test]
    fn time_rejects_empty_fraction() {
        let expected: bool = false;
        let actual: bool = Format::Time.is_valid("08:30:06.Z");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_accepts_all_elements() {
        let expected: bool = true;
        let actual: bool = Format::Duration.is_valid("P1Y2M3DT4H5M6S");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_accepts_week_form() {
        let expected: bool = true;
        let actual: bool = Format::Duration.is_valid("P4W");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_accepts_time_only() {
        let expected: bool = true;
        let actual: bool = Format::Duration.is_valid("PT36H");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_accepts_month_and_day() {
        let expected: bool = true;
        let actual: bool = Format::Duration.is_valid("P2M3D");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_rejects_week_mixed_with_other_units() {
        let expected: bool = false;
        let actual: bool = Format::Duration.is_valid("P1Y2W");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_rejects_week_with_time() {
        let expected: bool = false;
        let actual: bool = Format::Duration.is_valid("P1WT1H");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_rejects_no_elements() {
        let expected: bool = false;
        let actual: bool = Format::Duration.is_valid("P");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_rejects_time_separator_without_element() {
        let expected: bool = false;
        let actual: bool = Format::Duration.is_valid("P1YT");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_rejects_out_of_order_elements() {
        let expected: bool = false;
        let actual: bool = Format::Duration.is_valid("P2D1Y");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_rejects_year_and_day_without_month() {
        let expected: bool = false;
        let actual: bool = Format::Duration.is_valid("P1Y1D");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_rejects_hour_and_second_without_minute() {
        let expected: bool = false;
        let actual: bool = Format::Duration.is_valid("PT1H1S");
        assert_eq!(expected, actual);
    }

    #[test]
    fn duration_rejects_element_without_unit() {
        let expected: bool = false;
        let actual: bool = Format::Duration.is_valid("P1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_accepts_dot_atom() {
        let expected: bool = true;
        let actual: bool = Format::Email.is_valid("joe.bloggs@example.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_accepts_quoted_local_part_with_space_and_at() {
        let expected: bool = true;
        let actual: bool = Format::Email.is_valid("\"joe @bloggs\"@example.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_accepts_ipv4_literal_domain() {
        let expected: bool = true;
        let actual: bool = Format::Email.is_valid("joe.bloggs@[127.0.0.1]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_accepts_ipv6_literal_domain() {
        let expected: bool = true;
        let actual: bool = Format::Email.is_valid("joe.bloggs@[IPv6:::1]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_rejects_missing_at() {
        let expected: bool = false;
        let actual: bool = Format::Email.is_valid("2962");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_rejects_consecutive_dots_in_local_part() {
        let expected: bool = false;
        let actual: bool = Format::Email.is_valid("te..st@example.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_rejects_trailing_dot_in_local_part() {
        let expected: bool = false;
        let actual: bool = Format::Email.is_valid("test.@example.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_rejects_invalid_domain() {
        let expected: bool = false;
        let actual: bool = Format::Email.is_valid("joe.bloggs@invalid=domain.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_rejects_out_of_range_ipv4_literal() {
        let expected: bool = false;
        let actual: bool = Format::Email.is_valid("joe.bloggs@[127.0.0.300]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn email_rejects_non_ascii_local_part() {
        let expected: bool = false;
        let actual: bool = Format::Email.is_valid("실례@example.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_email_accepts_non_ascii_local_part_and_domain() {
        let expected: bool = true;
        let actual: bool = Format::IdnEmail.is_valid("실례@실례.테스트");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_email_rejects_missing_at() {
        let expected: bool = false;
        let actual: bool = Format::IdnEmail.is_valid("2962");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_accepts_multiple_labels() {
        let expected: bool = true;
        let actual: bool = Format::Hostname.is_valid("www.example.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_accepts_63_character_label() {
        let expected: bool = true;
        let actual: bool = Format::Hostname
            .is_valid("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijk.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_rejects_64_character_label() {
        let expected: bool = false;
        let actual: bool = Format::Hostname
            .is_valid("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijkl.com");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_rejects_trailing_dot() {
        let expected: bool = false;
        let actual: bool = Format::Hostname.is_valid("example.");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_rejects_leading_hyphen() {
        let expected: bool = false;
        let actual: bool = Format::Hostname.is_valid("-hostname");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_rejects_underscore() {
        let expected: bool = false;
        let actual: bool = Format::Hostname.is_valid("host_name");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_accepts_valid_punycode_label() {
        let expected: bool = true;
        let actual: bool = Format::Hostname.is_valid("xn--9n2bp8q.xn--9t4b11yi5a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_rejects_invalid_punycode_label() {
        let expected: bool = false;
        let actual: bool = Format::Hostname.is_valid("xn--X");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_rejects_punycode_decoding_to_double_hyphen_in_third_position() {
        let expected: bool = false;
        let actual: bool = Format::Hostname.is_valid("XN--aa---o47jg78q");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_rejects_punycode_decoding_to_ascii_only() {
        let expected: bool = false;
        let actual: bool = Format::Hostname.is_valid("xn--abc-");
        assert_eq!(expected, actual);
    }

    #[test]
    fn hostname_rejects_punycode_decoding_to_leading_combining_mark() {
        let expected: bool = false;
        let actual: bool = Format::Hostname.is_valid("xn--hello-zed");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_hangul() {
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("실례.테스트");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_ideographic_full_stop_separator() {
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("a\u{3002}b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_trailing_fullwidth_full_stop() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("example\u{ff0e}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_disallowed_code_point() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("실\u{302e}례.테스트");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_uppercase_non_ascii() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("\u{c9}cole");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_leading_combining_mark() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("\u{300}hello");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_middle_dot_between_ls() {
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("l\u{b7}l");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_middle_dot_without_following_l() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("l\u{b7}a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_greek_keraia_before_greek() {
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("\u{3b1}\u{375}\u{3b2}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_greek_keraia_before_latin() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("\u{3b1}\u{375}S");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_hebrew_geresh_after_hebrew() {
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("\u{5d0}\u{5f3}\u{5d1}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_hebrew_gershayim_after_latin() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("A\u{5f4}\u{5d1}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_katakana_middle_dot_with_hiragana() {
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("\u{30fb}\u{3041}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_katakana_middle_dot_without_kana_or_han() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("def\u{30fb}abc");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_mixed_arabic_indic_digits() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("\u{628}\u{660}\u{6f0}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_zero_width_joiner_after_virama() {
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("\u{915}\u{94d}\u{200d}\u{937}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_zero_width_joiner_without_virama() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("\u{915}\u{200d}\u{937}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_zero_width_non_joiner_between_joining_letters() {
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("\u{628}\u{64a}\u{200c}\u{628}\u{64a}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_zero_width_non_joiner_between_latin_letters() {
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("a\u{200c}b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_rejects_label_whose_a_label_exceeds_63_characters() {
        // The A-label is 65 characters long.
        let expected: bool = false;
        let actual: bool = Format::IdnHostname.is_valid("실례테스트가나다라마바사아자차카타파하실");
        assert_eq!(expected, actual);
    }

    #[test]
    fn idn_hostname_accepts_label_whose_a_label_fits_63_characters() {
        // The A-label is 62 characters long.
        let expected: bool = true;
        let actual: bool = Format::IdnHostname.is_valid("실례테스트가나다라마바사아자차카타파하");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv4_accepts_dotted_quad() {
        let expected: bool = true;
        let actual: bool = Format::Ipv4.is_valid("192.168.0.1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv4_rejects_leading_zero() {
        let expected: bool = false;
        let actual: bool = Format::Ipv4.is_valid("087.10.0.1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv4_rejects_octet_above_255() {
        let expected: bool = false;
        let actual: bool = Format::Ipv4.is_valid("256.256.256.256");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv4_rejects_three_octets() {
        let expected: bool = false;
        let actual: bool = Format::Ipv4.is_valid("127.0.0");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv4_rejects_netmask() {
        let expected: bool = false;
        let actual: bool = Format::Ipv4.is_valid("192.168.1.0/24");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_accepts_eight_groups() {
        let expected: bool = true;
        let actual: bool = Format::Ipv6.is_valid("1:2:3:4:5:6:7:8");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_accepts_compression_alone() {
        let expected: bool = true;
        let actual: bool = Format::Ipv6.is_valid("::");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_accepts_compression_in_middle() {
        let expected: bool = true;
        let actual: bool = Format::Ipv6.is_valid("1:d6::42");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_accepts_ipv4_tail() {
        let expected: bool = true;
        let actual: bool = Format::Ipv6.is_valid("::ffff:192.168.0.1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_rejects_two_compressions() {
        let expected: bool = false;
        let actual: bool = Format::Ipv6.is_valid("1::d6::42");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_rejects_compression_covering_no_group() {
        let expected: bool = false;
        let actual: bool = Format::Ipv6.is_valid("1:2:3:4::5:6:7:8");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_rejects_seven_groups_without_compression() {
        let expected: bool = false;
        let actual: bool = Format::Ipv6.is_valid("1:2:3:4:5:6:7");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_rejects_five_digit_group() {
        let expected: bool = false;
        let actual: bool = Format::Ipv6.is_valid("::abcef");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_rejects_zone_id() {
        let expected: bool = false;
        let actual: bool = Format::Ipv6.is_valid("fe80::a%eth1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_rejects_ipv4_tail_before_compression() {
        let expected: bool = false;
        let actual: bool = Format::Ipv6.is_valid("1.2.3.4::1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_accepts_absolute_uri() {
        let expected: bool = true;
        let actual: bool = Format::Uri.is_valid("http://foo.bar/?baz=qux#quux");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_rejects_relative_reference() {
        let expected: bool = false;
        let actual: bool = Format::Uri.is_valid("/abc");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_rejects_non_ascii() {
        let expected: bool = false;
        let actual: bool = Format::Uri.is_valid("https://example.org/foobar\u{ae}.txt");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_reference_accepts_relative_reference() {
        let expected: bool = true;
        let actual: bool = Format::UriReference.is_valid("/abc");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_reference_rejects_backslash() {
        let expected: bool = false;
        let actual: bool = Format::UriReference.is_valid("#frag\\ment");
        assert_eq!(expected, actual);
    }

    #[test]
    fn iri_accepts_non_ascii() {
        let expected: bool = true;
        let actual: bool = Format::Iri
            .is_valid("http://\u{192}\u{f8}\u{f8}.\u{df}\u{e5}r/?\u{2202}=\u{3c0}#\u{3c0}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn iri_rejects_relative_reference() {
        let expected: bool = false;
        let actual: bool = Format::Iri.is_valid("\u{e2}\u{3c0}\u{3c0}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn iri_reference_accepts_relative_non_ascii_reference() {
        let expected: bool = true;
        let actual: bool = Format::IriReference.is_valid("/\u{e2}\u{3c0}\u{3c0}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn iri_reference_rejects_backslash() {
        let expected: bool = false;
        let actual: bool = Format::IriReference.is_valid("#\u{192}r\u{e4}g\\m\u{ea}nt");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_template_accepts_expressions_with_modifiers() {
        let expected: bool = true;
        let actual: bool =
            Format::UriTemplate.is_valid("http://example.com/{+path}/{term:1}/{list*}{?a,b.c}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_template_accepts_template_without_expressions() {
        let expected: bool = true;
        let actual: bool = Format::UriTemplate.is_valid("http://example.com/dictionary");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_template_rejects_unclosed_expression() {
        let expected: bool = false;
        let actual: bool =
            Format::UriTemplate.is_valid("http://example.com/dictionary/{term:1}/{term");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_template_rejects_unopened_expression() {
        let expected: bool = false;
        let actual: bool = Format::UriTemplate.is_valid("http://example.com/term}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_template_rejects_prefix_of_zero() {
        let expected: bool = false;
        let actual: bool = Format::UriTemplate.is_valid("{term:0}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_template_rejects_prefix_above_9999() {
        let expected: bool = false;
        let actual: bool = Format::UriTemplate.is_valid("{term:10000}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uri_template_rejects_empty_expression() {
        let expected: bool = false;
        let actual: bool = Format::UriTemplate.is_valid("{}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_pointer_accepts_empty_pointer() {
        let expected: bool = true;
        let actual: bool = Format::JsonPointer.is_valid("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_pointer_accepts_escapes() {
        let expected: bool = true;
        let actual: bool = Format::JsonPointer.is_valid("/foo/bar~0/baz~1/%a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_pointer_rejects_unescaped_tilde() {
        let expected: bool = false;
        let actual: bool = Format::JsonPointer.is_valid("/foo/bar~");
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_pointer_rejects_missing_leading_slash() {
        let expected: bool = false;
        let actual: bool = Format::JsonPointer.is_valid("a/a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_pointer_rejects_uri_fragment() {
        let expected: bool = false;
        let actual: bool = Format::JsonPointer.is_valid("#/");
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_json_pointer_accepts_upward_then_downward() {
        let expected: bool = true;
        let actual: bool = Format::RelativeJsonPointer.is_valid("2/0/baz/1/zip");
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_json_pointer_accepts_member_name_reference() {
        let expected: bool = true;
        let actual: bool = Format::RelativeJsonPointer.is_valid("0#");
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_json_pointer_accepts_index_manipulation() {
        let expected: bool = true;
        let actual: bool = Format::RelativeJsonPointer.is_valid("0-1/foo");
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_json_pointer_rejects_leading_zero() {
        let expected: bool = false;
        let actual: bool = Format::RelativeJsonPointer.is_valid("01/a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_json_pointer_rejects_sign_without_prefix() {
        let expected: bool = false;
        let actual: bool = Format::RelativeJsonPointer.is_valid("+1/foo/bar");
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_json_pointer_rejects_double_hash() {
        let expected: bool = false;
        let actual: bool = Format::RelativeJsonPointer.is_valid("0##");
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_json_pointer_rejects_empty_string() {
        let expected: bool = false;
        let actual: bool = Format::RelativeJsonPointer.is_valid("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn regex_accepts_ecmascript_pattern() {
        let expected: bool = true;
        let actual: bool = Format::Regex.is_valid("([abc])+\\s+$");
        assert_eq!(expected, actual);
    }

    #[test]
    fn regex_rejects_unclosed_group() {
        let expected: bool = false;
        let actual: bool = Format::Regex.is_valid("^(abc]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn regex_rejects_identity_escape_of_letter() {
        let expected: bool = false;
        let actual: bool = Format::Regex.is_valid("\\a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uuid_accepts_mixed_case() {
        let expected: bool = true;
        let actual: bool = Format::Uuid.is_valid("2eb8aa08-AA98-11ea-B4Aa-73B441D16380");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uuid_rejects_missing_hyphens() {
        let expected: bool = false;
        let actual: bool = Format::Uuid.is_valid("2eb8aa0811ea-b4aa-73b441d16380----");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uuid_rejects_non_hex_digit() {
        let expected: bool = false;
        let actual: bool = Format::Uuid.is_valid("2eb8aa08-aa98-11ea-b4ga-73b441d16380");
        assert_eq!(expected, actual);
    }
}
