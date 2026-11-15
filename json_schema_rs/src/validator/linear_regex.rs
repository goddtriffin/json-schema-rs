//! Translating ECMA-262 patterns for the linear-time engine ([`RegexEngine::Linear`]).
//!
//! The `regex` crate matches in time linear in the input, so no pattern can make it backtrack
//! exponentially, but its syntax and semantics differ from ECMA-262's. A pattern is first
//! compiled as ECMA-262 (so the linear engine accepts exactly the patterns the default engine
//! accepts, or fewer), then its `u`-valid form is translated construct by construct into a
//! `regex` pattern that matches the same strings:
//!
//! - `.` excludes ECMA-262's four line terminators, not only `\n`;
//! - `\d`, `\w`, `\b` are ASCII and `\s` is ECMA-262's whitespace set, as classes written out;
//! - every literal becomes a `\x{…}` escape, so no character means something else in `regex`
//!   (`#`, `&`, `~`, `-` in classes);
//! - groups become non-capturing (capture names follow other rules in `regex`, and matching
//!   does not read captures);
//! - surrogate escapes pair into the code point they encode, and a lone surrogate, which no
//!   Rust string contains, matches nothing.
//!
//! Backreferences and lookaround have no linear-time implementation, so a pattern using them is
//! refused with [`Unsupported`] rather than translated into something that matches differently.
//!
//! [`RegexEngine::Linear`]: super::RegexEngine::Linear

use super::ecma_regex;
use std::fmt::Write;

/// Why a valid ECMA-262 pattern has no linear-time translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unsupported {
    Backreference,
    Lookaround,
    /// A Unicode property name `regex` does not know.
    Property,
}

impl Unsupported {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Unsupported::Backreference => "backreferences are not supported by the linear engine",
            Unsupported::Lookaround => "lookaround is not supported by the linear engine",
            Unsupported::Property => "this Unicode property is not supported by the linear engine",
        }
    }
}

/// `pattern`, a valid ECMA-262 pattern, compiled for the linear-time engine, its compiled program
/// held to `size_limit` bytes when one is set. The error says why the engine refused it.
pub(crate) fn compile(pattern: &str, size_limit: Option<usize>) -> Result<regex::Regex, String> {
    let translated: String = translate(&ecma_regex::u_form(pattern))
        .map_err(|unsupported| unsupported.as_str().to_string())?;
    regex::RegexBuilder::new(&translated)
        .size_limit(size_limit.unwrap_or(usize::MAX))
        .dfa_size_limit(size_limit.unwrap_or(usize::MAX))
        .nest_limit(u32::MAX)
        .build()
        .map_err(|error| match error {
            regex::Error::CompiledTooBig(limit) => {
                format!("the compiled pattern exceeds the size limit of {limit} bytes")
            }
            other => other.to_string(),
        })
}

/// ECMA-262 `\s`: `WhiteSpace` and `LineTerminator`.
const SPACE: &str = r"\x{9}-\x{D}\x{20}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";
const DIGIT: &str = r"0-9";
const WORD: &str = r"0-9A-Za-z_";
/// What `.` does not match without the `s` flag: ECMA-262's `LineTerminator`s.
const LINE_TERMINATORS: &str = r"\x{A}\x{D}\x{2028}\x{2029}";
/// A class that matches nothing.
const NOTHING: &str = r"[^\x{0}-\x{10FFFF}]";
/// A class that matches any code point.
const ANYTHING: &str = r"[\x{0}-\x{10FFFF}]";

fn is_surrogate(c: u32) -> bool {
    (0xD800..=0xDFFF).contains(&c)
}

/// One element of a class: a code point, or a class escape written as a nested `regex` class.
enum ClassAtom {
    Char(u32),
    Class(String),
    /// An unescaped `-`, a range operator between two code points and a literal otherwise.
    Dash,
}

/// A scanner over the `u`-valid pattern.
struct Scanner<'p> {
    pattern: &'p str,
    at: usize,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<char> {
        self.pattern[self.at..].chars().next()
    }

    fn next(&mut self) -> Option<char> {
        let c: char = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, prefix: &str) -> bool {
        let found: bool = self.pattern[self.at..].starts_with(prefix);
        if found {
            self.at += prefix.len();
        }
        found
    }

    /// `n` hexadecimal digits.
    fn hex(&mut self, n: usize) -> Option<u32> {
        let digits: &str = self.pattern.get(self.at..self.at + n)?;
        let value: u32 = u32::from_str_radix(digits, 16).ok()?;
        self.at += n;
        Some(value)
    }

    /// The code point of a `\u` escape after the `u` (`XXXX` or `{X…}`), pairing a high
    /// surrogate with a `\uXXXX` low surrogate right after it, as the `u` flag does.
    fn unicode_escape(&mut self) -> u32 {
        if self.eat("{") {
            let end: usize = self.pattern[self.at..]
                .find('}')
                .map_or(self.pattern.len(), |e| self.at + e);
            let value: u32 = u32::from_str_radix(&self.pattern[self.at..end], 16).unwrap_or(0xD800);
            self.at = end + 1;
            return value;
        }
        let high: u32 = self.hex(4).unwrap_or(0xD800);
        if (0xD800..=0xDBFF).contains(&high) && self.pattern[self.at..].starts_with("\\u") {
            let saved: usize = self.at;
            self.at += 2;
            match self.hex(4) {
                Some(low) if (0xDC00..=0xDFFF).contains(&low) => {
                    return 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
                }
                _ => self.at = saved,
            }
        }
        high
    }

    /// The `{…}` body of `\p` / `\P`, translated into `regex`'s spelling.
    fn property(&mut self) -> Result<String, Unsupported> {
        self.eat("{");
        let end: usize = self.pattern[self.at..]
            .find('}')
            .map_or(self.pattern.len(), |e| self.at + e);
        let body: &str = &self.pattern[self.at..end];
        self.at = end + 1;
        // ECMA-262 names the key `General_Category`, `gc`, `Script`, `sc`,
        // `Script_Extensions` or `scx`; `regex` takes all of them (with `=` or `:`).
        let ok: bool = regex::Regex::new(&format!(r"\p{{{body}}}")).is_ok();
        if ok {
            Ok(body.to_string())
        } else {
            Err(Unsupported::Property)
        }
    }
}

/// A code point as a `regex` literal.
fn literal(out: &mut String, c: u32) {
    if is_surrogate(c) {
        out.push_str(NOTHING);
    } else if let Some(ch) = char::from_u32(c).filter(char::is_ascii_alphanumeric) {
        out.push(ch);
    } else {
        write!(out, r"\x{{{c:X}}}").expect("writing to a String");
    }
}

/// A code point inside a class (never a surrogate).
fn class_char(out: &mut String, c: u32) {
    if let Some(ch) = char::from_u32(c).filter(char::is_ascii_alphanumeric) {
        out.push(ch);
    } else {
        write!(out, r"\x{{{c:X}}}").expect("writing to a String");
    }
}

/// The code point of a `CharacterEscape` after its `\` and letter `c`, or `None` when `c`
/// starts a class escape or something else.
fn character_escape(scanner: &mut Scanner<'_>, c: char) -> Option<u32> {
    Some(match c {
        'f' => 0xC,
        'n' => 0xA,
        'r' => 0xD,
        't' => 0x9,
        'v' => 0xB,
        '0' => 0,
        'c' => u32::from(scanner.next()?) % 32,
        'x' => scanner.hex(2)?,
        'u' => scanner.unicode_escape(),
        // An identity escape of a syntax character, `/` or (in a class) `-`.
        other => u32::from(other),
    })
}

/// The nested class for a class escape (`\d`, `\S`, `\p{L}`), or `None` when `c` is not one.
fn class_escape(scanner: &mut Scanner<'_>, c: char) -> Option<Result<String, Unsupported>> {
    let (set, negated): (&str, bool) = match c {
        'd' => (DIGIT, false),
        'D' => (DIGIT, true),
        'w' => (WORD, false),
        'W' => (WORD, true),
        's' => (SPACE, false),
        'S' => (SPACE, true),
        'p' | 'P' => {
            return Some(scanner.property().map(|body| format!(r"\{c}{{{body}}}")));
        }
        _ => return None,
    };
    Some(Ok(format!("[{}{set}]", if negated { "^" } else { "" })))
}

/// `pattern`, a pattern the `u` grammar accepts, as a `regex` pattern that matches the same
/// strings.
pub(crate) fn translate(pattern: &str) -> Result<String, Unsupported> {
    let mut out: String = String::with_capacity(pattern.len() * 2);
    let mut scanner: Scanner<'_> = Scanner { pattern, at: 0 };
    while let Some(c) = scanner.next() {
        match c {
            '\\' => {
                let Some(e) = scanner.next() else { break };
                match e {
                    'b' => out.push_str(r"(?-u:\b)"),
                    'B' => out.push_str(r"(?-u:\B)"),
                    '1'..='9' | 'k' => return Err(Unsupported::Backreference),
                    _ => {
                        if let Some(class) = class_escape(&mut scanner, e) {
                            out.push_str(&class?);
                        } else if let Some(code) = character_escape(&mut scanner, e) {
                            literal(&mut out, code);
                        }
                    }
                }
            }
            '(' => {
                if scanner.eat("?:") {
                    out.push_str("(?:");
                } else if scanner.eat("?=")
                    || scanner.eat("?!")
                    || scanner.eat("?<=")
                    || scanner.eat("?<!")
                {
                    return Err(Unsupported::Lookaround);
                } else if scanner.eat("?<") {
                    let end: usize = pattern[scanner.at..]
                        .find('>')
                        .map_or(pattern.len(), |e| scanner.at + e);
                    scanner.at = end + 1;
                    out.push_str("(?:");
                } else {
                    out.push_str("(?:");
                }
            }
            '.' => write!(out, "[^{LINE_TERMINATORS}]").expect("writing to a String"),
            '[' => class(&mut scanner, &mut out)?,
            // Quantifiers (`{n,m}` keeps its digits), anchors, alternation and `)` mean the same.
            '^' | '$' | '|' | ')' | '*' | '+' | '?' | '{' | '}' | ',' | '0'..='9' => out.push(c),
            other => literal(&mut out, u32::from(other)),
        }
    }
    Ok(out)
}

/// The class starting after its `[`, written to `out`.
fn class(scanner: &mut Scanner<'_>, out: &mut String) -> Result<(), Unsupported> {
    let negated: bool = scanner.eat("^");
    let mut atoms: Vec<ClassAtom> = Vec::new();
    while let Some(c) = scanner.next() {
        match c {
            ']' => break,
            '-' => atoms.push(ClassAtom::Dash),
            '\\' => {
                let Some(e) = scanner.next() else { break };
                if e == 'b' {
                    atoms.push(ClassAtom::Char(0x8));
                } else if let Some(class) = class_escape(scanner, e) {
                    atoms.push(ClassAtom::Class(class?));
                } else if let Some(code) = character_escape(scanner, e) {
                    atoms.push(ClassAtom::Char(code));
                }
            }
            other => atoms.push(ClassAtom::Char(u32::from(other))),
        }
    }
    let mut body: String = String::new();
    let mut i: usize = 0;
    while i < atoms.len() {
        match (&atoms[i], atoms.get(i + 1), atoms.get(i + 2)) {
            (ClassAtom::Char(lo), Some(ClassAtom::Dash), Some(ClassAtom::Char(hi))) => {
                // The scalar values of `lo..=hi`, without the surrogates.
                for (from, to) in [(*lo, (*hi).min(0xD7FF)), ((*lo).max(0xE000), *hi)] {
                    if from <= to {
                        class_char(&mut body, from);
                        body.push('-');
                        class_char(&mut body, to);
                    }
                }
                i += 3;
            }
            (ClassAtom::Char(c), _, _) => {
                if !is_surrogate(*c) {
                    class_char(&mut body, *c);
                }
                i += 1;
            }
            (ClassAtom::Dash, _, _) => {
                body.push_str(r"\x{2D}");
                i += 1;
            }
            (ClassAtom::Class(nested), _, _) => {
                body.push_str(nested);
                i += 1;
            }
        }
    }
    match (body.is_empty(), negated) {
        (true, false) => out.push_str(NOTHING),
        (true, true) => out.push_str(ANYTHING),
        (false, _) => {
            out.push('[');
            if negated {
                out.push('^');
            }
            out.push_str(&body);
            out.push(']');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Unsupported, compile, translate};
    use crate::validator::ecma_regex;

    /// Whether `pattern` matches somewhere in each of `texts`, under the ECMA-262 engine and
    /// under the linear engine (`None` when the linear engine refuses it).
    fn both(pattern: &str, texts: &[&str]) -> (Vec<bool>, Option<Vec<bool>>) {
        let ecma: regress::Regex = ecma_regex::compile(pattern).expect("valid ECMA-262");
        let linear: Option<regex::Regex> = compile(pattern, None).ok();
        (
            texts.iter().map(|t| ecma.find(t).is_some()).collect(),
            linear.map(|r| texts.iter().map(|t| r.is_match(t)).collect()),
        )
    }

    /// The two engines agree on `pattern` for every text.
    fn agrees(pattern: &str, texts: &[&str]) {
        let (ecma, linear) = both(pattern, texts);
        assert_eq!(Some(ecma), linear, "{pattern}");
    }

    const TEXTS: &[&str] = &[
        "",
        "a",
        "A",
        "abc",
        "a-b",
        "0",
        "09",
        "x\ny",
        "x\ry",
        "x\u{2028}y",
        "\u{85}",
        "\u{FEFF}",
        " ",
        "\t",
        "é",
        "ж",
        "ab_9",
        "😀",
        "a😀b",
        "{}",
        "#&~",
        "[]",
        "^$",
        "a,b",
        "aaa",
        "-",
        "\u{8}",
        "\u{0}",
        "/",
        "\\",
    ];

    #[test]
    fn linear_engine_agrees_with_ecma_engine() {
        for pattern in [
            "^a",
            "c$",
            "^$",
            "a|b",
            "a.b",
            "^.$",
            "\\d",
            "\\D",
            "\\w+",
            "\\W",
            "\\s",
            "\\S",
            "\\bab",
            "b\\B",
            "[a-c]",
            "[^a-c]",
            "[-a]",
            "[a-]",
            "[\\-]",
            "[\\d-]",
            "[\\w#&~]",
            "[]",
            "[^]",
            "[\\b]",
            "\\x41",
            "\\u0041",
            "\\u{1F600}",
            "\\uD83D\\uDE00",
            "[\\uD83D\\uDE00]",
            "\\uD83D",
            "[\\uD800-\\uFFFF]",
            "\\cJ",
            "\\0",
            "a{2}",
            "a{1,}",
            "a{1,2}?",
            "(a)(?:b)?",
            "(?<name>a)",
            "\\p{L}",
            "\\P{L}",
            "\\p{Lu}",
            "\\p{Script=Greek}",
            "\\p{sc=Cyrillic}",
            "\\p{gc=Nd}",
            "[\\p{L}\\d]",
            "#",
            "&",
            "~",
            "\\/",
            "\\.",
            "\\\\",
            "\\[\\]",
            "a,b",
            "\\{\\}",
            "\\t",
            "\\n",
            "\\v\\f\\r",
            "^[^\\s]+$",
            "😀",
            "[😀-😂]",
            "^\\]$",
            "\\-",
            "a\\:b",
        ] {
            agrees(pattern, TEXTS);
        }
    }

    #[test]
    fn linear_engine_refuses_backreference() {
        let expected: Result<String, Unsupported> = Err(Unsupported::Backreference);
        let actual: Result<String, Unsupported> = translate(r"(a)\1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn linear_engine_refuses_named_backreference() {
        let expected: Result<String, Unsupported> = Err(Unsupported::Backreference);
        let actual: Result<String, Unsupported> = translate(r"(?<n>a)\k<n>");
        assert_eq!(expected, actual);
    }

    #[test]
    fn linear_engine_refuses_lookahead() {
        let expected: Result<String, Unsupported> = Err(Unsupported::Lookaround);
        let actual: Result<String, Unsupported> = translate(r"a(?=b)");
        assert_eq!(expected, actual);
    }

    #[test]
    fn linear_engine_refuses_lookbehind() {
        let expected: Result<String, Unsupported> = Err(Unsupported::Lookaround);
        let actual: Result<String, Unsupported> = translate(r"(?<!a)b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn translate_writes_literals_as_escapes() {
        let expected: Result<String, Unsupported> =
            Ok(r"^a\x{23}[\x{26}\x{7E}\x{2D}]+$".to_string());
        let actual: Result<String, Unsupported> = translate(r"^a#[&~-]+$");
        assert_eq!(expected, actual);
    }
}
