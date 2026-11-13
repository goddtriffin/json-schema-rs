//! A non-allocating matcher for the patterns in a regular subset of ECMA-262, tried before the
//! regular expression engine.
//!
//! The subset: ASCII character sets (a literal, `.`, `\d` / `\w` / `\s` and their negations, an
//! escaped punctuation character, or a bracket class of those and ranges), groups (`(...)`,
//! `(?:...)`), alternation, and the quantifiers `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}` (greedy or
//! lazy), with `^` only at the start and `$` only at the end of a top-level alternative. Without
//! backreferences, lookaround or word boundaries, whether a pattern matches somewhere in a text
//! does not depend on the order a backtracking engine tries its paths: it is a question about a
//! regular language, which a Glushkov automaton (one state per character-set occurrence, at most
//! 128 so a state set is one `u128`) decides in one pass over the text, without allocating. The
//! matcher refuses any other pattern. It answers only while the text it reads is ASCII; on a
//! non-ASCII byte it gives no answer and the caller asks the regular expression engine, so every
//! Unicode rule stays with `regress`.

use super::compiled::IntMap;

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
                // Only digits and one comma form a quantifier (`str::parse` would also take a
                // sign).
                if !body.bytes().all(|b| b.is_ascii_digit() || b == b',') {
                    return None;
                }
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

/// The most states an automaton may have: one bit each in a `u128` state set.
const MAX_STATES: usize = 128;

/// The deepest group nesting the parser follows (it recurses once per level).
const MAX_DEPTH: u32 = 32;

/// A pattern (or part of one) in the subset, as parsed.
#[derive(Debug)]
enum Regex {
    Set(AsciiSet),
    Concat(Vec<Regex>),
    Alt(Vec<Regex>),
    /// `inner` repeated `min` to `max` times (`max` is `u32::MAX` for no bound).
    Repeat {
        inner: Box<Regex>,
        min: u32,
        max: u32,
    },
}

impl Reader<'_> {
    /// The alternatives of a group's body, up to its `)` (not consumed).
    fn alternation(&mut self, depth: u32) -> Option<Regex> {
        let mut alternatives: Vec<Regex> = vec![self.sequence(depth)?];
        while self.eat(b'|') {
            alternatives.push(self.sequence(depth)?);
        }
        Some(if alternatives.len() == 1 {
            alternatives.swap_remove(0)
        } else {
            Regex::Alt(alternatives)
        })
    }

    /// A sequence of quantified atoms, up to a `|`, a `)` or the end. At the top level
    /// (`depth` 0) it also stops at `^` and `$`, which [`SimplePattern::parse`] places;
    /// inside a group they are outside the subset.
    fn sequence(&mut self, depth: u32) -> Option<Regex> {
        let mut items: Vec<Regex> = Vec::new();
        loop {
            let atom: Regex = match self.peek() {
                None | Some(b'|' | b')') => break,
                Some(b'^' | b'$') if depth == 0 => break,
                Some(_) => self.atom(depth)?,
            };
            let quantified: bool = matches!(self.peek(), Some(b'*' | b'+' | b'?' | b'{'));
            let (min, max): (u32, u32) = self.quantifier()?;
            if quantified {
                // Laziness changes which match is found, not whether one exists.
                self.eat(b'?');
            }
            // Stacked quantifiers are outside the subset.
            if min > max || matches!(self.peek(), Some(b'*' | b'+' | b'?' | b'{')) {
                return None;
            }
            items.push(if (min, max) == (1, 1) {
                atom
            } else {
                Regex::Repeat {
                    inner: Box::new(atom),
                    min,
                    max,
                }
            });
        }
        Some(Regex::Concat(items))
    }

    /// One atom: a character set or a group.
    fn atom(&mut self, depth: u32) -> Option<Regex> {
        let set: AsciiSet = match self.bump()? {
            b'(' => {
                // `(?=`, `(?!`, `(?<=`, `(?<!` and named groups are outside the subset.
                if self.eat(b'?') && !self.eat(b':') {
                    return None;
                }
                if depth >= MAX_DEPTH {
                    return None;
                }
                let inner: Regex = self.alternation(depth + 1)?;
                return self.eat(b')').then_some(inner);
            }
            b'.' => DOT,
            b'\\' => escape(self.bump()?)?,
            b'[' => self.class()?,
            b'*' | b'+' | b'?' | b'{' | b'}' | b']' | b'^' | b'$' => return None,
            c => AsciiSet::single(c),
        };
        Some(Regex::Set(set))
    }
}

/// What a part of a pattern contributes to the automaton: whether it matches the empty string,
/// and the states it can start and end in.
#[derive(Debug, Clone, Copy)]
struct Fragment {
    nullable: bool,
    first: u128,
    last: u128,
}

impl Fragment {
    const EMPTY: Fragment = Fragment {
        nullable: true,
        first: 0,
        last: 0,
    };
}

/// The indexes of the set bits of `bits`, lowest first.
fn states(mut bits: u128) -> impl Iterator<Item = usize> {
    std::iter::from_fn(move || {
        (bits != 0).then(|| {
            let state: usize = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            state
        })
    })
}

/// Builds the Glushkov automaton of a parsed pattern: one state per character-set occurrence
/// (each copy of a repeated part is its own), and for each state the states that can follow it.
#[derive(Debug, Default)]
struct Builder {
    sets: Vec<AsciiSet>,
    follow: Vec<u128>,
}

impl Builder {
    fn build(&mut self, regex: &Regex) -> Option<Fragment> {
        match regex {
            Regex::Set(set) => {
                if self.sets.len() == MAX_STATES {
                    return None;
                }
                let bit: u128 = 1 << self.sets.len();
                self.sets.push(*set);
                self.follow.push(0);
                Some(Fragment {
                    nullable: false,
                    first: bit,
                    last: bit,
                })
            }
            Regex::Concat(items) => items.iter().try_fold(Fragment::EMPTY, |fragment, item| {
                let next: Fragment = self.build(item)?;
                Some(self.concat(fragment, next))
            }),
            Regex::Alt(alternatives) => {
                alternatives
                    .iter()
                    .try_fold(None, |union: Option<Fragment>, alternative| {
                        let next: Fragment = self.build(alternative)?;
                        Some(Some(match union {
                            None => next,
                            Some(union) => Fragment {
                                nullable: union.nullable || next.nullable,
                                first: union.first | next.first,
                                last: union.last | next.last,
                            },
                        }))
                    })?
            }
            Regex::Repeat { inner, min, max } => self.repeat(inner, *min, *max),
        }
    }

    /// `inner{min,max}` as `min` required copies then `max - min` optional ones, or, without
    /// an upper bound, `max(min, 1)` copies whose last repeats.
    fn repeat(&mut self, inner: &Regex, min: u32, max: u32) -> Option<Fragment> {
        let before: usize = self.sets.len();
        let first_copy: Fragment = self.build(inner)?;
        let size: usize = self.sets.len() - before;
        let copies: u32 = if max == u32::MAX { min.max(1) } else { max };
        // A part without character sets matches only the empty string, however often it
        // repeats; zero copies match only the empty string too.
        if size == 0 || copies == 0 {
            return Some(Fragment::EMPTY);
        }
        if usize::try_from(copies).ok()?.checked_mul(size)? > MAX_STATES - before {
            return None;
        }
        let mut fragment: Fragment = Fragment::EMPTY;
        for k in 0..copies {
            let mut copy: Fragment = if k == 0 {
                first_copy
            } else {
                self.build(inner)?
            };
            if k >= min {
                copy.nullable = true;
            }
            if max == u32::MAX && k + 1 == copies {
                for state in states(copy.last) {
                    self.follow[state] |= copy.first;
                }
            }
            fragment = self.concat(fragment, copy);
        }
        Some(fragment)
    }

    fn concat(&mut self, left: Fragment, right: Fragment) -> Fragment {
        for state in states(left.last) {
            self.follow[state] |= right.first;
        }
        Fragment {
            nullable: left.nullable && right.nullable,
            first: left.first | if left.nullable { right.first } else { 0 },
            last: right.last | if right.nullable { left.last } else { 0 },
        }
    }
}

/// The most states a determinized automaton may have (its states are `u8`s).
const MAX_DFA_STATES: usize = 256;

/// The state before the first byte.
const START: u8 = 0;
/// The state no match can come out of any more.
const DEAD: u8 = 1;
/// The state reached once a match has been found.
const MATCHED: u8 = 2;

/// A pattern in the subset the automaton decides (see the module docs), as a deterministic
/// automaton over ASCII bytes: the Glushkov automaton determinized when the schema compiles,
/// with a search restarting at every byte, so matching reads each byte once with one lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SimplePattern {
    /// The state after reading a byte: `next[state * 128 + byte]`.
    next: Box<[u8]>,
    /// The states in which the text read so far ends a match, one bit each.
    accepting: [u128; 2],
    /// Whether an alternative matches the empty string: then the empty text matches, and
    /// every text does unless that alternative has both `^` and `$`.
    matches_empty: bool,
    matches_all: bool,
}

/// The automaton of a parsed pattern, before it is determinized: where a match can start
/// and end, at the edges of the text and anywhere else.
struct Automaton {
    follow: Vec<u128>,
    /// For each ASCII byte, the states whose set contains it.
    accepts: [u128; 128],
    /// The states a match can start in at the start of the text, and anywhere else (those
    /// of the alternatives without `^`).
    first_at_start: u128,
    first_anywhere: u128,
    /// The states a match can end in at the end of the text, and anywhere else (those of the
    /// alternatives without `$`).
    last_at_end: u128,
    last_anywhere: u128,
}

/// Subset construction over [`Automaton`]: each deterministic state stands for the set of
/// automaton states the live match attempts are in.
struct Determinizer<'a> {
    automaton: &'a Automaton,
    /// The bytes no state tells apart share a class: each byte's class, and each class's
    /// states (those whose set contains its bytes).
    class_of: [u8; 128],
    classes: Vec<u128>,
    sets: Vec<u128>,
    ids: IntMap<u128, u8>,
    /// The state of the empty set, the most common target, once it is known.
    empty: Option<u8>,
}

impl Determinizer<'_> {
    /// The deterministic state for live attempts in `set`, adding it when it is new.
    fn state(&mut self, set: u128) -> Option<u8> {
        if set == 0 && self.empty.is_some() {
            return self.empty;
        }
        if set & self.automaton.last_anywhere != 0 {
            return Some(MATCHED);
        }
        if set == 0 && self.automaton.first_anywhere == 0 {
            self.empty = Some(DEAD);
            return Some(DEAD);
        }
        if let Some(id) = self.ids.get(&set) {
            return Some(*id);
        }
        if self.sets.len() == MAX_DFA_STATES {
            return None;
        }
        let id: u8 = u8::try_from(self.sets.len()).ok()?;
        self.sets.push(set);
        self.ids.insert(set, id);
        if set == 0 {
            self.empty = Some(id);
        }
        Some(id)
    }

    /// The transitions of a state whose attempts can next be in `reachable`.
    fn row(&mut self, reachable: u128, next: &mut Vec<u8>) -> Option<()> {
        let mut targets: [u8; 128] = [DEAD; 128];
        for class in 0..self.classes.len() {
            targets[class] = self.state(reachable & self.classes[class])?;
        }
        next.extend(
            self.class_of
                .iter()
                .map(|class| targets[usize::from(*class)]),
        );
        Some(())
    }

    fn run(mut self) -> Option<(Box<[u8]>, [u128; 2])> {
        // `START`, `DEAD` and `MATCHED` take the first three ids; they stand for no set.
        self.sets = vec![0, 0, 0];
        let mut next: Vec<u8> = Vec::with_capacity(16 * 128);
        self.row(self.automaton.first_at_start, &mut next)?;
        next.extend(std::iter::repeat_n(DEAD, 128));
        next.extend(std::iter::repeat_n(MATCHED, 128));
        let mut accepting: [u128; 2] = [0; 2];
        accepting[0] |= 1 << MATCHED;
        let mut id: usize = 3;
        while let Some(set) = self.sets.get(id).copied() {
            if set & self.automaton.last_at_end != 0 {
                accepting[id / 128] |= 1 << (id % 128);
            }
            let reachable: u128 = states(set)
                .fold(self.automaton.first_anywhere, |union, state| {
                    union | self.automaton.follow[state]
                });
            self.row(reachable, &mut next)?;
            id += 1;
        }
        Some((next.into_boxed_slice(), accepting))
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
        let mut builder: Builder = Builder::default();
        let mut automaton: Automaton = Automaton {
            follow: Vec::new(),
            accepts: [0; 128],
            first_at_start: 0,
            first_anywhere: 0,
            last_at_end: 0,
            last_anywhere: 0,
        };
        let mut matches_empty: bool = false;
        let mut matches_all: bool = false;
        // Each top-level alternative: `^`, a sequence, `$`, then `|` or the end.
        loop {
            let at_start: bool = reader.eat(b'^');
            let body: Regex = reader.sequence(0)?;
            let at_end: bool = reader.eat(b'$');
            let fragment: Fragment = builder.build(&body)?;
            automaton.first_at_start |= fragment.first;
            automaton.last_at_end |= fragment.last;
            if !at_start {
                automaton.first_anywhere |= fragment.first;
            }
            if !at_end {
                automaton.last_anywhere |= fragment.last;
            }
            matches_empty |= fragment.nullable;
            matches_all |= fragment.nullable && !(at_start && at_end);
            match reader.bump() {
                None => break,
                Some(b'|') => {}
                Some(_) => return None,
            }
        }
        for (state, set) in builder.sets.iter().enumerate() {
            for byte in states(set.0) {
                automaton.accepts[byte] |= 1 << state;
            }
        }
        automaton.follow = builder.follow;
        let mut class_of: [u8; 128] = [0; 128];
        let mut classes: Vec<u128> = Vec::new();
        for (byte, states) in automaton.accepts.iter().enumerate() {
            let class: usize = classes.iter().position(|c| c == states).unwrap_or_else(|| {
                classes.push(*states);
                classes.len() - 1
            });
            class_of[byte] = u8::try_from(class).ok()?;
        }
        let (next, accepting): (Box<[u8]>, [u128; 2]) = Determinizer {
            automaton: &automaton,
            class_of,
            classes,
            sets: Vec::new(),
            ids: IntMap::default(),
            empty: None,
        }
        .run()?;
        Some(SimplePattern {
            next,
            accepting,
            matches_empty,
            matches_all,
        })
    }

    /// Whether the pattern matches somewhere in `text`, or `None` when the matcher read a
    /// non-ASCII byte before deciding (the caller then asks the regular expression engine).
    pub(crate) fn is_match(&self, text: &str) -> Option<bool> {
        if self.matches_all {
            return Some(true);
        }
        let bytes: &[u8] = text.as_bytes();
        if bytes.is_empty() {
            return Some(self.matches_empty);
        }
        let mut state: u8 = START;
        for byte in bytes {
            if !byte.is_ascii() {
                return None;
            }
            state = self.next[usize::from(state) * 128 + usize::from(*byte)];
            // No byte leads back to `START`, so this is `DEAD` or `MATCHED`.
            if state <= MATCHED {
                return Some(state == MATCHED);
            }
        }
        let id: usize = usize::from(state);
        Some((self.accepting[id / 128] >> (id % 128)) & 1 == 1)
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

    /// The automaton's answer for `pattern` (which must qualify) against `text`.
    fn simple(pattern: &str, text: &str) -> Option<bool> {
        SimplePattern::parse(pattern)
            .expect("test pattern qualifies")
            .is_match(text)
    }

    #[test]
    fn qualifying_patterns_parse() {
        let patterns: [&str; 22] = [
            "^",
            "$",
            "^$",
            "",
            "abc",
            "[a-z]+$",
            "^[0-9]+\\.[0-9]+\\.[0-9]+$",
            "^CVE-\\d{4}-\\d{4,7}$",
            "^[\\]-]$",
            "^a{0}b$",
            "^(when|unless)$",
            ".+",
            "^a|b$",
            "^(?:ab)+$",
            "^a*?b$",
            "^a+a$",
            "^(a|)$",
            "^()*$",
            "\\d+(\\.\\d+)?[mhs]",
            "^(10|[1-9])m|([1-9][0-9]*)s$",
            "^a{2,}?$",
            "(((a)))",
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
        let patterns: [&str; 20] = [
            "[ab]*a[ab]{9}",
            "^\\bword$",
            "^\\u0041$",
            "^é$",
            "^[]$",
            "^[^]$",
            "^a$b",
            "a^",
            "(^a)",
            "(a$)",
            "^a{2,1}$",
            "^a{x}$",
            "^a{+2}$",
            "(?=a)",
            "(?<name>a)",
            "(a)\\1",
            "a)",
            "a{129}",
            "(a{100}){2}",
            "((((((((((((((((((((((((((((((((((a))))))))))))))))))))))))))))))))))",
        ];
        let expected: Vec<bool> = vec![false; patterns.len()];
        let actual: Vec<bool> = patterns
            .iter()
            .map(|p| SimplePattern::parse(p).is_some())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn matches_and_mismatches() {
        let expected: [Option<bool>; 10] = [
            Some(true),
            Some(false),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
        ];
        let actual: [Option<bool>; 10] = [
            simple("^CVE-\\d{4}-\\d{4,7}$", "CVE-2024-12345"),
            simple("^CVE-\\d{4}-\\d{4,7}$", "CVE-2024-123"),
            simple("^(when|unless)$", "unless"),
            simple("^(when|unless)$", "run"),
            simple(".+", "main"),
            simple(".+", "\n"),
            simple("^a+a$", "aa"),
            simple("^a+a$", "a"),
            simple("\\d+(\\.\\d+)?[mhs]", "wait 1.5s"),
            simple("^(10|[1-9])m|([1-9][0-9]*)s$", "0s0"),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn pattern_matching_empty_string_decides_without_reading() {
        let expected: [Option<bool>; 3] = [Some(true), Some(true), Some(false)];
        let actual: [Option<bool>; 3] = [simple("a*", "é"), simple("^$", ""), simple("^$", "a")];
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
    const PROPERTY_PATTERNS: [&str; 58] = [
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
        "",
        "$",
        "^$",
        "abc",
        "b+$",
        "^(a|b)$",
        "^a|b$",
        "^a*?b$",
        "^a+a$",
        "^[a-z]*[0-9a]$",
        "(a|bc)*c",
        "^(?:ab|a)(?:bc|c)?$",
        "^(a|)+b",
        "\\d+(\\.\\d+)?[a-c]",
        "^(1|[1-9])a|([1-9][0-9]*)b$",
        "^(x{2,3}|y){1,2}$",
        ".+",
        "^(a?){2}a{2}$",
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
