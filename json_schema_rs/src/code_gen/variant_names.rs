//! Variant names that pass `clippy::enum_variant_names` (see design.md, *Lint-clean output*).
//!
//! The lint (default `enum-variant-name-threshold` = 3, so only enums of three or more variants)
//! fires when every variant shares a leading or a trailing camel-case word (`OptOne`, `OptTwo`,
//! `OptThree`), and on a variant that starts or ends with its enum's own name (`KindFoo` in
//! `Kind`). Generated code may not carry a lint attribute, so the names themselves change:
//! the shared words are dropped, and each variant keeps its schema value through serde's
//! `rename` (string enums) or its own literal (value enums), so what the JSON reads and writes
//! never changes.

use super::rust_backend::unique_idents;
use crate::sanitizers::RUST_KEYWORD_TYPE_SELF;

/// Clippy's default `enum-variant-name-threshold`: enums with fewer variants are not checked.
const THRESHOLD: usize = 3;

/// How many rounds of stripping are tried before the positional fallback. Each round shortens
/// names, so a handful always suffices; the bound only makes termination obvious.
const MAX_ROUNDS: usize = 16;

/// Stand-ins for a variant that is its enum's own name, tried in order.
const REPLACEMENTS: [&str; 3] = ["Value", "Member", "Case"];

/// The words of `name`: a word starts at every uppercase letter. Finer than clippy's own split
/// (it keeps a run of capitals together, though a name from a schema value only has runs of
/// single-letter words, `ENOne` from `e_n_one`), so any word prefix or suffix clippy sees every
/// variant share is one here too.
fn words(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current: String = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        current.push(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Prefixes tried, in order, for a name a cut made invalid.
const ESCAPES: [&str; 4] = ["E", "V", "X", "Z"];

/// A name usable as a variant after a cut: a leading digit gets `E`, like any enum value that
/// starts with one, and `Self` (the one keyword in `PascalCase`) becomes `ESelf`. The prefix is
/// another letter when `E` would make the name start with the enum's own name (`E` in `E`).
fn valid_ident(enum_name: &str, name: &str) -> String {
    if !name.chars().next().is_some_and(|c| c.is_ascii_digit()) && name != RUST_KEYWORD_TYPE_SELF {
        return name.to_string();
    }
    let mut last: String = String::new();
    for escape in ESCAPES {
        last = format!("{escape}{name}");
        if starts_with_enum(enum_name, &last).is_none()
            && ends_with_enum(enum_name, &last).is_none()
        {
            break;
        }
    }
    last
}

/// Clippy's `variant name starts with the enum's name`: the enum's name, then a character that
/// is not lowercase, then one that is not a digit (a name that ends there is not reported).
fn starts_with_enum(enum_name: &str, name: &str) -> Option<String> {
    let rest: &str = name.strip_prefix(enum_name)?;
    let mut chars = rest.chars();
    let first: char = chars.next()?;
    let second: char = chars.next()?;
    (!first.is_lowercase() && !second.is_numeric()).then(|| rest.to_string())
}

/// Clippy's `variant name ends with the enum's name`: the name's tail is the enum's name.
fn ends_with_enum(enum_name: &str, name: &str) -> Option<String> {
    name.strip_suffix(enum_name).map(str::to_string)
}

/// A name for a variant that was only its enum's name: a word that neither starts nor ends
/// with it.
fn replacement(enum_name: &str) -> String {
    REPLACEMENTS
        .iter()
        .find(|word| !word.starts_with(enum_name) && !word.ends_with(enum_name))
        .map_or_else(|| "Variant".to_string(), |word| (*word).to_string())
}

/// Drops the enum's name from the start or end of every variant that has it.
fn strip_enum_name(enum_name: &str, names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|name| {
            if let Some(rest) = starts_with_enum(enum_name, name) {
                valid_ident(enum_name, &rest)
            } else if let Some(rest) = ends_with_enum(enum_name, name) {
                if rest.is_empty() {
                    replacement(enum_name)
                } else {
                    valid_ident(enum_name, &rest)
                }
            } else {
                name.clone()
            }
        })
        .collect()
}

/// Drops the leading, else the trailing, words every variant starts (ends) with, keeping at
/// least one word per variant. `None` when no word is shared.
fn strip_shared_words(enum_name: &str, names: &[String]) -> Option<Vec<String>> {
    let split: Vec<Vec<String>> = names.iter().map(|n| words(n)).collect();
    // A variant of one word breaks every shared run, as in clippy, which also reads a name that
    // ends in a digit (`NMinus1`) as one word.
    if names
        .iter()
        .any(|n| n.ends_with(|c: char| c.is_ascii_digit()))
    {
        return None;
    }
    let shortest: usize = split.iter().map(Vec::len).min()?;
    if shortest < 2 {
        return None;
    }
    let first: &Vec<String> = split.first()?;
    let prefix: usize = (0..shortest)
        .take_while(|&i| split.iter().all(|w| w.get(i) == first.get(i)))
        .count();
    if prefix > 0 {
        let cut: usize = prefix.min(shortest - 1);
        return Some(
            split
                .iter()
                .map(|w| {
                    valid_ident(
                        enum_name,
                        &w.iter().skip(cut).map(String::as_str).collect::<String>(),
                    )
                })
                .collect(),
        );
    }
    let suffix: usize = (1..=shortest)
        .take_while(|&i| {
            split
                .iter()
                .all(|w| w.iter().rev().nth(i - 1) == first.iter().rev().nth(i - 1))
        })
        .count();
    if suffix > 0 {
        let cut: usize = suffix.min(shortest - 1);
        return Some(
            split
                .iter()
                .map(|w| {
                    valid_ident(
                        enum_name,
                        &w.iter()
                            .take(w.len() - cut)
                            .map(String::as_str)
                            .collect::<String>(),
                    )
                })
                .collect(),
        );
    }
    None
}

/// Makes `names` (the variants of enum `enum_name`, unique) pass `clippy::enum_variant_names`.
/// Variants that start or end with the enum's name lose it; words every variant starts or ends
/// with are dropped (keeping one word each); names the cuts make equal get numeric suffixes
/// (`Foo0`, `Foo1`) like any collision. The result has the same length and order, unique
/// names, each a valid identifier; an enum clippy would not report comes back unchanged.
pub(super) fn lint_clean_variant_names(enum_name: &str, names: Vec<String>) -> Vec<String> {
    let mut names: Vec<String> = names;
    if names.len() < THRESHOLD {
        return names;
    }
    for _ in 0..MAX_ROUNDS {
        let stripped: Vec<String> = strip_enum_name(enum_name, &names);
        let cut: Vec<String> =
            strip_shared_words(enum_name, &stripped).unwrap_or_else(|| stripped.clone());
        if cut == names {
            return names;
        }
        names = unique_idents(&cut, "");
    }
    // Unreachable in practice (every round shortens the names): positional names are always clean.
    (0..names.len()).map(|i| format!("Variant{i}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(enum_name: &str, names: &[&str]) -> Vec<String> {
        lint_clean_variant_names(enum_name, names.iter().map(|n| (*n).to_string()).collect())
    }

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn words_split_on_lower_to_upper() {
        let expected: Vec<String> = strings(&["Opt", "One"]);
        let actual: Vec<String> = words("OptOne");
        assert_eq!(expected, actual);
    }

    #[test]
    fn words_split_at_every_capital() {
        let expected: Vec<String> = strings(&["H", "T", "T", "P", "Server"]);
        let actual: Vec<String> = words("HTTPServer");
        assert_eq!(expected, actual);
    }

    #[test]
    fn words_split_after_a_digit() {
        let expected: Vec<String> = strings(&["Opt1", "Two"]);
        let actual: Vec<String> = words("Opt1Two");
        assert_eq!(expected, actual);
    }

    #[test]
    fn shared_leading_word_is_dropped() {
        let expected: Vec<String> = strings(&["One", "Two", "Three"]);
        let actual: Vec<String> = clean("Kind", &["OptOne", "OptTwo", "OptThree"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn shared_trailing_word_is_dropped() {
        let expected: Vec<String> = strings(&["One", "Two", "Three"]);
        let actual: Vec<String> = clean("Kind", &["OneOpt", "TwoOpt", "ThreeOpt"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn shared_acronym_is_dropped() {
        let expected: Vec<String> = strings(&["One", "Two", "Three"]);
        let actual: Vec<String> = clean("Kind", &["HTTPOne", "HTTPTwo", "HTTPThree"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn several_shared_leading_words_are_dropped_keeping_one_word_each() {
        let expected: Vec<String> = strings(&["One", "OneTwo", "OneThree"]);
        let actual: Vec<String> = clean("Kind", &["OptOne", "OptOneTwo", "OptOneThree"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn variant_starting_with_enum_name_loses_it() {
        let expected: Vec<String> = strings(&["Foo", "Other", "Third"]);
        let actual: Vec<String> = clean("Kind", &["KindFoo", "Other", "Third"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn variant_ending_with_enum_name_loses_it() {
        let expected: Vec<String> = strings(&["Foo", "Other", "Third"]);
        let actual: Vec<String> = clean("Kind", &["FooKind", "Other", "Third"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn variant_equal_to_enum_name_is_replaced() {
        let expected: Vec<String> = strings(&["Value", "Other", "Third"]);
        let actual: Vec<String> = clean("Kind", &["Kind", "Other", "Third"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn variant_equal_to_enum_name_value_is_replaced_by_another_word() {
        let expected: Vec<String> = strings(&["Member", "Other", "Third"]);
        let actual: Vec<String> = clean("Value", &["Value", "Other", "Third"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_name_cut_leaving_a_digit_gets_e_prefix() {
        let expected: Vec<String> = strings(&["E1Foo", "Other", "Third"]);
        let actual: Vec<String> = clean("Kind", &["Kind1Foo", "Other", "Third"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_name_cut_leaving_self_gets_e_prefix() {
        let expected: Vec<String> = strings(&["ESelf", "Other", "Third"]);
        let actual: Vec<String> = clean("Kind", &["KindSelf", "Other", "Third"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn shared_word_cut_leaving_self_gets_e_prefix() {
        let expected: Vec<String> = strings(&["ESelf", "One", "Two"]);
        let actual: Vec<String> = clean("Kind", &["OptSelf", "OptOne", "OptTwo"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn names_equal_after_the_cut_get_numeric_suffixes() {
        let expected: Vec<String> = strings(&["Foo0", "Foo1", "Other"]);
        let actual: Vec<String> = clean("Kind", &["KindFoo", "Foo", "Other"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_of_two_variants_is_unchanged() {
        let expected: Vec<String> = strings(&["OptOne", "OptTwo"]);
        let actual: Vec<String> = clean("Kind", &["OptOne", "OptTwo"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn variant_of_one_word_keeps_the_names() {
        let expected: Vec<String> = strings(&["Opt", "OptOne", "OptTwo"]);
        let actual: Vec<String> = clean("Kind", &["Opt", "OptOne", "OptTwo"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn names_ending_in_a_digit_are_kept() {
        let expected: Vec<String> = strings(&["N1Point5", "N2Point5", "N3Point5"]);
        let actual: Vec<String> = clean("Kind", &["N1Point5", "N2Point5", "N3Point5"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn names_without_shared_words_are_unchanged() {
        let expected: Vec<String> = strings(&["OptOne", "AltTwo", "BetaThree"]);
        let actual: Vec<String> = clean("Kind", &["OptOne", "AltTwo", "BetaThree"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_word_names_are_unchanged() {
        let expected: Vec<String> = strings(&["Red", "Green", "Blue"]);
        let actual: Vec<String> = clean("Kind", &["Red", "Green", "Blue"]);
        assert_eq!(expected, actual);
    }
}
