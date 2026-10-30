//! A non-allocating matcher for the most common `pattern` shapes, tried before the regular
//! expression engine.
//!
//! It accepts a pattern anchored with `^` (and optionally `$`) that is a sequence of ASCII
//! character sets (a literal, `.`, `\d` / `\w` / `\s` and their negations, an escaped punctuation
//! character, or a bracket class of those and ranges), each with an optional greedy quantifier
//! (`*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}`). Matching is greedy and never backtracks, which is
//! exact only when every repeated set is disjoint from each set that can follow it, up to and
//! including the next one that must match: the matcher refuses any other pattern. It answers only
//! while the text it reads is ASCII; on a non-ASCII byte it gives no answer and the caller asks
//! the regular expression engine, so every Unicode rule stays with `regress`.

/// A set of ASCII characters, one bit per code point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AsciiSet(u128);

impl AsciiSet {
    const EMPTY: AsciiSet = AsciiSet(0);

    const fn single(c: u8) -> AsciiSet {
        AsciiSet::range(c, c)
    }

    /// Every character from `lo` to `hi` inclusive (both ASCII).
    const fn range(lo: u8, hi: u8) -> AsciiSet {
        let upper: u128 = if hi >= 127 {
            u128::MAX
        } else {
            (1_u128 << (hi + 1)) - 1
        };
        let lower: u128 = (1_u128 << lo) - 1;
        AsciiSet(upper & !lower)
    }

    const fn union(self, other: AsciiSet) -> AsciiSet {
        AsciiSet(self.0 | other.0)
    }

    const fn complement(self) -> AsciiSet {
        AsciiSet(!self.0)
    }

    const fn contains(self, byte: u8) -> bool {
        byte < 128 && (self.0 >> byte) & 1 == 1
    }

    const fn intersects(self, other: AsciiSet) -> bool {
        self.0 & other.0 != 0
    }
}

const DIGIT: AsciiSet = AsciiSet::range(b'0', b'9');
const WORD: AsciiSet = DIGIT
    .union(AsciiSet::range(b'a', b'z'))
    .union(AsciiSet::range(b'A', b'Z'))
    .union(AsciiSet::single(b'_'));
/// ECMA-262 `\s` within ASCII: TAB, LF, VT, FF, CR and SPACE.
const SPACE: AsciiSet = AsciiSet::range(9, 13).union(AsciiSet::single(b' '));
/// `.`: everything but the ASCII line terminators LF and CR.
const DOT: AsciiSet = AsciiSet::single(b'\n')
    .union(AsciiSet::single(b'\r'))
    .complement();

/// One set with its repetition bounds (`max` is `u32::MAX` for no bound).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Item {
    set: AsciiSet,
    min: u32,
    max: u32,
}

/// A pattern in the subset the greedy matcher decides exactly (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SimplePattern {
    items: Box<[Item]>,
    /// Whether the pattern ends with `$`.
    anchored_end: bool,
}

/// The set an escape stands for: `\d`, `\w`, `\s`, their negations, or an escaped ASCII
/// punctuation character. Any other escape is outside the subset.
fn escape(c: u8) -> Option<AsciiSet> {
    match c {
        b'd' => Some(DIGIT),
        b'w' => Some(WORD),
        b's' => Some(SPACE),
        b'D' => Some(DIGIT.complement()),
        b'W' => Some(WORD.complement()),
        b'S' => Some(SPACE.complement()),
        c if c.is_ascii_punctuation() => Some(AsciiSet::single(c)),
        _ => None,
    }
}

/// A byte-level reader over the pattern source.
struct Reader<'s> {
    bytes: &'s [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte: Option<u8> = self.peek();
        self.at += 1;
        byte
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found: bool = self.peek() == Some(byte);
        if found {
            self.at += 1;
        }
        found
    }

    /// A bracket class, after its `[`.
    fn class(&mut self) -> Option<AsciiSet> {
        let negated: bool = self.eat(b'^');
        let mut set: AsciiSet = AsciiSet::EMPTY;
        // `[]` and `[^]` are the empty and the full class: outside the subset.
        if self.peek() == Some(b']') {
            return None;
        }
        loop {
            let lo: u8 = match self.bump()? {
                b']' => break,
                b'[' => return None,
                b'\\' => {
                    let escaped: u8 = self.bump()?;
                    if escaped.is_ascii_punctuation() {
                        escaped
                    } else {
                        set = set.union(escape(escaped)?);
                        continue;
                    }
                }
                c => c,
            };
            let ranged: bool = self.peek() == Some(b'-')
                && self.bytes.get(self.at + 1).is_some_and(|b| *b != b']');
            if ranged {
                let hi: u8 = self.bytes[self.at + 1];
                if hi == b'\\' || hi == b'[' || hi < lo {
                    return None;
                }
                self.at += 2;
                set = set.union(AsciiSet::range(lo, hi));
            } else {
                set = set.union(AsciiSet::single(lo));
            }
        }
        Some(if negated { set.complement() } else { set })
    }

    /// The quantifier after an atom, as `(min, max)`; `(1, 1)` when there is none.
    fn quantifier(&mut self) -> Option<(u32, u32)> {
        let bounds: (u32, u32) = match self.peek() {
            Some(b'*') => (0, u32::MAX),
            Some(b'+') => (1, u32::MAX),
            Some(b'?') => (0, 1),
            Some(b'{') => {
                let close: usize =
                    self.at + self.bytes[self.at..].iter().position(|b| *b == b'}')?;
                let body: &str = std::str::from_utf8(&self.bytes[self.at + 1..close]).ok()?;
                let bounds: (u32, u32) = match body.split_once(',') {
                    None => {
                        let n: u32 = body.parse().ok()?;
                        (n, n)
                    }
                    Some((lo, "")) => (lo.parse().ok()?, u32::MAX),
                    Some((lo, hi)) => (lo.parse().ok()?, hi.parse().ok()?),
                };
                self.at = close;
                bounds
            }
            _ => return Some((1, 1)),
        };
        self.at += 1;
        Some(bounds)
    }
}

impl SimplePattern {
    /// The pattern, when it is in the subset this matcher decides exactly.
    pub(crate) fn parse(source: &str) -> Option<SimplePattern> {
        if !source.is_ascii() {
            return None;
        }
        let mut reader: Reader<'_> = Reader {
            bytes: source.as_bytes(),
            at: 0,
        };
        // An unanchored pattern is a search: outside the subset.
        if !reader.eat(b'^') {
            return None;
        }
        let mut items: Vec<Item> = Vec::new();
        let mut anchored_end: bool = false;
        while let Some(c) = reader.bump() {
            let set: AsciiSet = match c {
                b'$' if reader.peek().is_none() => {
                    anchored_end = true;
                    break;
                }
                b'.' => DOT,
                b'\\' => escape(reader.bump()?)?,
                b'[' => reader.class()?,
                b'(' | b')' | b'|' | b'*' | b'+' | b'?' | b'{' | b'}' | b'^' | b'$' | b']' => {
                    return None;
                }
                c => AsciiSet::single(c),
            };
            let (min, max): (u32, u32) = reader.quantifier()?;
            // Lazy and stacked quantifiers are outside the subset.
            if min > max || matches!(reader.peek(), Some(b'?' | b'*' | b'+' | b'{')) {
                return None;
            }
            items.push(Item { set, min, max });
        }
        let greedy_is_exact: bool = items.iter().enumerate().all(|(k, item)| {
            item.min == item.max
                || items[k + 1..]
                    .iter()
                    .scan(false, |reached_required, next| {
                        // Stop after the first later item that must match.
                        (!*reached_required).then(|| {
                            *reached_required = next.min > 0;
                            next
                        })
                    })
                    .all(|next| !item.set.intersects(next.set))
        });
        greedy_is_exact.then(|| SimplePattern {
            items: items.into_boxed_slice(),
            anchored_end,
        })
    }

    /// Whether `text` matches, or `None` when the matcher read a non-ASCII byte (the caller
    /// then asks the regular expression engine).
    pub(crate) fn is_match(&self, text: &str) -> Option<bool> {
        let bytes: &[u8] = text.as_bytes();
        let mut at: usize = 0;
        for item in &self.items {
            let mut count: u32 = 0;
            while count < item.max {
                match bytes.get(at) {
                    Some(byte) if !byte.is_ascii() => return None,
                    Some(byte) if item.set.contains(*byte) => {
                        at += 1;
                        count += 1;
                    }
                    _ => break,
                }
            }
            if count < item.min {
                // Greedy is exact, so no other split of the ASCII prefix read so far matches.
                return Some(false);
            }
        }
        if !self.anchored_end {
            return Some(true);
        }
        match bytes.get(at) {
            None => Some(true),
            Some(byte) if !byte.is_ascii() => None,
            Some(_) => Some(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SimplePattern;

    /// Whether `pattern` matches `text`, as `regress` decides it (the `u` flag, as compiled).
    fn regress_matches(pattern: &str, text: &str) -> bool {
        regress::Regex::with_flags(pattern, "u")
            .expect("test patterns compile")
            .find(text)
            .is_some()
    }

    /// The greedy matcher's answer for `pattern` (which must qualify) against `text`.
    fn simple(pattern: &str, text: &str) -> Option<bool> {
        SimplePattern::parse(pattern)
            .expect("test pattern qualifies")
            .is_match(text)
    }

    #[test]
    fn qualifying_patterns_parse() {
        let patterns: [&str; 12] = [
            "^",
            "^/",
            "^[0-9]+\\.[0-9]+\\.[0-9]+$",
            "^[a-z][a-z0-9-]*$",
            "^[a-zA-Z0-9_-]+/[a-zA-Z0-9_-]+$",
            "^CVE-\\d{4}-\\d{4,7}$",
            "^[^*]+$",
            "^.+$",
            "^\\S*$",
            "^x{2,}y$",
            "^[\\]-]$",
            "^a{0}b$",
        ];
        let expected: Vec<bool> = vec![true; patterns.len()];
        let actual: Vec<bool> = patterns
            .iter()
            .map(|p| SimplePattern::parse(p).is_some())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_qualifying_patterns_are_refused() {
        let patterns: [&str; 16] = [
            "abc",
            "[a-z]+$",
            "^(a|b)$",
            "^a|b$",
            "^a*?b$",
            "^a+a$",
            "^a*b?a$",
            "^[a-z]*[0-9a]$",
            "^\\bword$",
            "^\\u0041$",
            "^é$",
            "^[]$",
            "^[^]$",
            "^a$b",
            "^a{2,1}$",
            "^a{x}$",
        ];
        let expected: Vec<bool> = vec![false; patterns.len()];
        let actual: Vec<bool> = patterns
            .iter()
            .map(|p| SimplePattern::parse(p).is_some())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn greedy_matches_and_mismatches() {
        let expected: [Option<bool>; 6] = [
            Some(true),
            Some(false),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
        ];
        let actual: [Option<bool>; 6] = [
            simple("^CVE-\\d{4}-\\d{4,7}$", "CVE-2024-12345"),
            simple("^CVE-\\d{4}-\\d{4,7}$", "CVE-2024-123"),
            simple("^[a-z][a-z0-9-]*$", "my-app-2"),
            simple("^[a-z][a-z0-9-]*$", "My-app"),
            simple("^/", "/path"),
            simple("^x{2,}y$", "xy"),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_ascii_text_falls_back() {
        let expected: [Option<bool>; 3] = [None, None, Some(true)];
        let actual: [Option<bool>; 3] = [
            simple("^.+$", "é"),
            simple("^[a-z]+$", "abcé"),
            // The match is decided before the non-ASCII byte is read.
            simple("^ab", "abé"),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn mismatch_before_non_ascii_text_is_decided() {
        let expected: Option<bool> = Some(false);
        let actual: Option<bool> = simple("^[0-9]+$", "xé");
        assert_eq!(expected, actual);
    }

    /// Patterns covering every construct of the subset, and near misses of it.
    const PROPERTY_PATTERNS: [&str; 40] = [
        "^",
        "^/",
        "^[0-9]+\\.[0-9]+\\.[0-9]+$",
        "^[a-z][a-z0-9-]*$",
        "^[A-Za-z][A-Za-z\\s\\d_-]*$",
        "^[a-zA-Z0-9_-]+/[a-zA-Z0-9_-]+$",
        "^[a-z][a-z\\d/_-]*$",
        "^[a-z_][a-z0-9_]*$",
        "^[a-zA-Z0-9_]+$",
        "^CVE-\\d{4}-\\d{4,7}$",
        "^[^*]+$",
        "^\\./",
        "^_",
        "^.+$",
        "^#.+$",
        "^[^.0-9]+$",
        "^a*b?a$",
        "^a*b?c$",
        "^[a]{1,2}b",
        "^\\S*$",
        "^\\w+$",
        "^x{2,}y$",
        "^[-a]$",
        "^[a-]$",
        "^[\\]]$",
        "^.*$",
        "^ab$",
        "^a.c",
        "^[^a-c]*d$",
        "^[\\d-]+$",
        "^\\D+$",
        "^[\\W]$",
        "^a{0}b$",
        "^\\s*x\\s*$",
        "^[ -~]+$",
        "^\\$\\d+$",
        "^a?b?c?$",
        "^[!-/]{2,3}z?$",
        "^\\.\\.?$",
        "^a{1,3}",
    ];

    /// The characters generated texts are drawn from: pattern-relevant ASCII, line terminators
    /// and non-ASCII.
    const PROPERTY_ALPHABET: &str = "ab c-_./*#$0123456789xyzABZ~!\n\r\t\u{b}]\\@é\u{a0}";

    /// A deterministic xorshift generator, so the property test is reproducible.
    struct XorShift(u64);

    impl XorShift {
        fn step(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, bound: usize) -> usize {
            usize::try_from(self.step() % u64::try_from(bound).expect("small bound"))
                .expect("below a usize bound")
        }
    }

    #[test]
    fn agrees_with_regress_on_generated_texts() {
        let alphabet: Vec<char> = PROPERTY_ALPHABET.chars().collect();
        let mut rng: XorShift = XorShift(0x1234_5678_9abc_def0);
        let mut disagreements: Vec<(String, String, bool)> = Vec::new();
        for pattern in PROPERTY_PATTERNS {
            let Some(simple) = SimplePattern::parse(pattern) else {
                continue;
            };
            for _ in 0..2_000 {
                let len: usize = rng.below(12);
                let text: String = (0..len)
                    .map(|_| alphabet[rng.below(alphabet.len())])
                    .collect();
                if let Some(matched) = simple.is_match(&text) {
                    let expected: bool = regress_matches(pattern, &text);
                    if matched != expected {
                        disagreements.push((pattern.to_string(), text, matched));
                    }
                }
            }
        }
        let expected: Vec<(String, String, bool)> = Vec::new();
        let actual: Vec<(String, String, bool)> = disagreements;
        assert_eq!(expected, actual);
    }
}
