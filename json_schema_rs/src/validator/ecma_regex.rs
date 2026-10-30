//! Compiling ECMA-262 regular expressions with Unicode semantics.
//!
//! Every pattern is compiled with the `u` flag, so `.` and classes match whole code points and
//! `\p{…}` / `\u{…}` keep their Unicode meaning. The `u` grammar is stricter than the one without
//! it, and patterns written without `u` in mind are valid ECMA-262 yet refused under it. When
//! `regress` refuses a pattern, it is rewritten once into an equivalent `u`-valid form and tried
//! again:
//!
//! - an identity escape of a character that is neither a syntax character, `/`, nor `ID_Continue`
//!   (`\-`, `\:`, `\"`, `\ `), valid without `u` (ECMA-262 `IdentityEscape[~U]`), becomes the
//!   code point escape `\u{…}`;
//! - a lone `]`, `{` or `}` (Annex B `ExtendedPatternCharacter`) becomes `\]`, `\{` or `\}`. A
//!   `{` that starts a braced quantifier is kept, so a quantifier with nothing to repeat stays an
//!   error, as it is in Annex B.
//!
//! Everything else is left as written, so constructs that are not valid ECMA-262 (`\a`, `\_`,
//! unbalanced groups) and the other Annex B extensions (quantified lookaheads, ranges with a class
//! escape, legacy octal escapes) remain errors, reported for the pattern as written.

use std::fmt::Write;
use std::sync::LazyLock;

/// Characters that may follow `\` under the `u` flag outside a class (ECMA-262
/// `SyntaxCharacter` and `/`).
fn is_u_identity_escape(c: char) -> bool {
    matches!(
        c,
        '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '/'
    )
}

/// Whether `c` has the Unicode property `ID_Continue`, as `regress` defines it (the Unicode
/// version it matches with). An identity escape of such a character is not valid ECMA-262.
fn is_id_continue(c: char) -> bool {
    static ID_CONTINUE: LazyLock<regress::Regex> = LazyLock::new(|| {
        regress::Regex::with_flags(r"^\p{ID_Continue}$", "u").expect("valid property escape")
    });
    let mut buffer: [u8; 4] = [0; 4];
    ID_CONTINUE.find(c.encode_utf8(&mut buffer)).is_some()
}

/// The length in bytes of the braced quantifier (`{n}`, `{n,}`, `{n,m}`) at the start of `rest`,
/// if one starts there.
fn braced_quantifier_len(rest: &str) -> Option<usize> {
    let bytes: &[u8] = rest.as_bytes();
    let digits = |from: usize| -> usize {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let lo: usize = digits(1);
    if lo == 0 {
        return None;
    }
    let mut at: usize = 1 + lo;
    if bytes.get(at) == Some(&b',') {
        at += 1;
        at += digits(at);
    }
    (bytes.get(at) == Some(&b'}')).then_some(at + 1)
}

/// `pattern` rewritten into its `u`-valid equivalent (see the module docs), or `None` when no
/// construct needed rewriting.
fn rewrite(pattern: &str) -> Option<String> {
    let mut out: String = String::with_capacity(pattern.len() + 8);
    let mut changed: bool = false;
    let mut in_class: bool = false;
    let mut at: usize = 0;
    while let Some(c) = pattern[at..].chars().next() {
        at += c.len_utf8();
        match c {
            '\\' => {
                let Some(escaped) = pattern[at..].chars().next() else {
                    out.push('\\');
                    break;
                };
                at += escaped.len_utf8();
                if is_u_identity_escape(escaped) || (in_class && escaped == '-') {
                    out.push('\\');
                    out.push(escaped);
                } else if is_id_continue(escaped) {
                    // A character, property or class escape (`\d`, `\u0041`, `\p{L}`, `\k<n>`), or
                    // an invalid identity escape (`\a`): kept for `regress` to decide. The braces
                    // of `\u{…}`, `\p{…}` and `\P{…}` belong to the escape.
                    out.push('\\');
                    out.push(escaped);
                    if matches!(escaped, 'u' | 'p' | 'P') && pattern[at..].starts_with('{') {
                        let end: usize = pattern[at..]
                            .find('}')
                            .map_or(pattern.len(), |close| at + close + 1);
                        out.push_str(&pattern[at..end]);
                        at = end;
                    }
                } else {
                    write!(out, "\\u{{{:X}}}", u32::from(escaped)).expect("writing to a String");
                    changed = true;
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            ']' | '}' if !in_class => {
                out.push('\\');
                out.push(c);
                changed = true;
            }
            '{' if !in_class => {
                if let Some(len) = braced_quantifier_len(&pattern[at - 1..]) {
                    out.push_str(&pattern[at - 1..at - 1 + len]);
                    at += len - 1;
                } else {
                    out.push_str("\\{");
                    changed = true;
                }
            }
            c => out.push(c),
        }
    }
    changed.then_some(out)
}

/// `pattern` compiled with Unicode semantics. A pattern the `u` grammar refuses is rewritten
/// (see the module docs) and compiled again; when that fails too, the error is the one for the
/// pattern as written.
pub(crate) fn compile(pattern: &str) -> Result<regress::Regex, regress::Error> {
    regress::Regex::with_flags(pattern, "u").or_else(|error| {
        rewrite(pattern)
            .and_then(|rewritten| regress::Regex::with_flags(&rewritten, "u").ok())
            .ok_or(error)
    })
}

#[cfg(test)]
mod tests {
    use super::{compile, rewrite};
    use std::ops::Range;

    /// Whether `pattern` compiles, and if so whether it finds a match in `text`.
    fn finds(pattern: &str, text: &str) -> Result<bool, String> {
        compile(pattern)
            .map(|regex| regex.find(text).is_some())
            .map_err(|error| error.to_string())
    }

    #[test]
    fn rewrite_escapes_identity_escape_outside_class() {
        let actual: Option<String> = rewrite(r"^a\-b\:c$");
        let expected: Option<String> = Some(r"^a\u{2D}b\u{3A}c$".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_escapes_identity_escape_inside_class() {
        let actual: Option<String> = rewrite(r#"[\:\"\-]"#);
        let expected: Option<String> = Some(r"[\u{3A}\u{22}\-]".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_escapes_astral_identity_escape() {
        let actual: Option<String> = rewrite("\\\u{1F600}");
        let expected: Option<String> = Some(r"\u{1F600}".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_escapes_lone_brackets_and_braces() {
        let actual: Option<String> = rewrite("^]a{b}{,1}$");
        let expected: Option<String> = Some(r"^\]a\{b\}\{,1\}$".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_keeps_braced_quantifiers() {
        let actual: Option<String> = rewrite(r"a{2}b{1,}c{1,3}\:");
        let expected: Option<String> = Some(r"a{2}b{1,}c{1,3}\u{3A}".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_keeps_braces_of_code_point_and_property_escapes() {
        let actual: Option<String> = rewrite(r"\u{3A}\p{L}\P{Lu}[\p{N}]]");
        let expected: Option<String> = Some(r"\u{3A}\p{L}\P{Lu}[\p{N}]\]".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_keeps_brackets_and_braces_inside_class() {
        let actual: Option<String> = rewrite(r"[{}[]");
        let expected: Option<String> = None;
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_keeps_id_continue_escapes() {
        let actual: Option<String> = rewrite(r"\a\_\e\d\cA\k<n>\1");
        let expected: Option<String> = None;
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_keeps_trailing_backslash() {
        let actual: Option<String> = rewrite(r"]\");
        let expected: Option<String> = Some(r"\]\".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn compile_reports_error_for_pattern_as_written() {
        let actual: Result<bool, String> = finds(r"\-(", "");
        let expected: Result<bool, String> = Err(regress::Regex::with_flags(r"\-(", "u")
            .expect_err("invalid under u")
            .to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn compile_rejects_quantifier_with_nothing_to_repeat() {
        let actual: bool = compile("{1}]").is_err();
        let expected: bool = true;
        assert_eq!(expected, actual);
    }

    #[test]
    fn compile_rejects_escape_turned_into_group_syntax() {
        // `(?\<a>b)` is invalid; the escape must not turn into the `(?<` of a named group.
        let actual: bool = compile(r"(?\<a>b)").is_err();
        let expected: bool = true;
        assert_eq!(expected, actual);
    }

    #[test]
    fn identity_escape_in_class_is_a_member_not_a_range() {
        let actual: Result<bool, String> = finds(r"^[a\:z]$", "m");
        let expected: Result<bool, String> = Ok(false);
        assert_eq!(expected, actual);
    }

    #[test]
    fn dot_matches_astral_code_point() {
        let actual: Result<bool, String> = finds(r"^.$", "\u{1F600}");
        let expected: Result<bool, String> = Ok(true);
        assert_eq!(expected, actual);
    }

    #[test]
    fn dot_matches_astral_code_point_next_to_rewritten_escape() {
        let actual: Result<bool, String> = finds(r"^\-.$", "-\u{1F600}");
        let expected: Result<bool, String> = Ok(true);
        assert_eq!(expected, actual);
    }

    #[test]
    fn property_escape_matches_letter() {
        let actual: Result<bool, String> = finds(r"^\p{L}\:$", "\u{e9}:");
        let expected: Result<bool, String> = Ok(true);
        assert_eq!(expected, actual);
    }

    #[test]
    fn property_escape_rejects_digit() {
        let actual: Result<bool, String> = finds(r"^\p{L}\:$", "1:");
        let expected: Result<bool, String> = Ok(false);
        assert_eq!(expected, actual);
    }

    #[test]
    fn code_point_escape_matches_astral_code_point() {
        let actual: Result<bool, String> = finds(r"^\u{1F600}\-$", "\u{1F600}-");
        let expected: Result<bool, String> = Ok(true);
        assert_eq!(expected, actual);
    }

    #[test]
    fn case_insensitive_group_folds_non_ascii() {
        let actual: Result<bool, String> = finds(r"^(?i:\u{e9})\-$", "\u{c9}-");
        let expected: Result<bool, String> = Ok(true);
        assert_eq!(expected, actual);
    }

    /// Every pattern of up to `TOKENS_LEN` tokens that compiles only after rewriting is valid
    /// without the `u` flag and matches every test text exactly as it does there: same match,
    /// same groups. On ASCII text and these tokens the two modes have the same meaning, so this
    /// checks the rewrite is an equivalence.
    #[test]
    fn rewritten_patterns_match_like_non_unicode_mode() {
        const TOKENS: [&str; 17] = [
            "a", "-", ":", "]", "[", "{", "}", "1", ",", "^", "$", "(", ")", "|", "*", r"\-", r"\:",
        ];
        const TOKENS_LEN: usize = 4;
        const TEXT_CHARS: [char; 7] = ['a', '-', ':', ']', '{', '}', '1'];
        let mut texts: Vec<String> = vec![String::new()];
        let mut layer: Vec<String> = vec![String::new()];
        for _ in 0..3 {
            layer = layer
                .iter()
                .flat_map(|prefix| TEXT_CHARS.iter().map(move |c| format!("{prefix}{c}")))
                .collect();
            texts.extend(layer.iter().cloned());
        }
        let groups = |regex: &regress::Regex, text: &str| -> Option<Vec<Option<Range<usize>>>> {
            regex.find(text).map(|m| m.groups().collect())
        };
        let mut mismatches: Vec<(String, String)> = Vec::new();
        let mut patterns: Vec<String> = vec![String::new()];
        for _ in 0..TOKENS_LEN {
            patterns = patterns
                .iter()
                .flat_map(|prefix| TOKENS.iter().map(move |t| format!("{prefix}{t}")))
                .collect();
            for pattern in &patterns {
                if regress::Regex::with_flags(pattern, "u").is_ok() {
                    continue;
                }
                let Ok(rewritten) = compile(pattern) else {
                    continue;
                };
                let Ok(original) = regress::Regex::with_flags(pattern, "") else {
                    mismatches.push((pattern.clone(), "invalid without u".to_string()));
                    continue;
                };
                for text in &texts {
                    if groups(&rewritten, text) != groups(&original, text) {
                        mismatches.push((pattern.clone(), text.clone()));
                    }
                }
            }
        }
        let expected: Vec<(String, String)> = Vec::new();
        assert_eq!(expected, mismatches);
    }
}
