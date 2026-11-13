//! Doc comments from schema annotations (`description`, `examples`), written so that generated
//! code passes the strictest lints a consumer can turn on: clippy's documentation lints
//! (`doc_markdown`, `doc_lazy_continuation`, `tabs_in_doc_comments`, ...) and rustdoc's
//! (`bare_urls`, `invalid_html_tags`, `broken_intra_doc_links`, doctests). Generated code is a
//! module of the consumer's crate, so it cannot opt out of their lints.
//!
//! A description is plain text. It is rendered as Markdown that shows the same text:
//! identifiers `doc_markdown` would flag go in backticks (found the way clippy finds them, so
//! nothing it checks is missed), URLs become autolinks, the characters Markdown and rustdoc read
//! as markup (`\`, `<`, `>`, `[`, `]`) are escaped, code spans and fenced blocks are kept (every
//! fence is labelled `text`, so none becomes a doctest), and a line after a list item that is not
//! one starts a new paragraph. When the rendered docs no longer read back as the description,
//! the item also carries `#[json_schema(description = "...")]`, which the derive prefers, so the
//! round trip through generated types stays lossless.

use serde_json::Value;
use std::fmt::Write as _;

/// The description as the round trip promises it: each line trimmed, runs of blank lines
/// collapsed to one, leading and trailing blank lines dropped. `None` when nothing is left.
pub(crate) fn normalized(description: &str) -> Option<String> {
    let lines: Vec<&str> = normalized_lines(description);
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn normalized_lines(description: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = Vec::new();
    for line in description.split('\n').map(str::trim) {
        if line.is_empty() && lines.last().is_none_or(|last: &&str| last.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|last: &&str| last.is_empty()) {
        lines.pop();
    }
    lines
}

/// What the derive reads back from doc lines: each line trimmed, joined with `\n`, then
/// trimmed (`derive_support::doc_description`).
pub(crate) fn read_back(lines: &[String]) -> Option<String> {
    let joined: String = lines
        .iter()
        .map(|line: &String| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");
    let trimmed: &str = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The doc comment lines for `description` (`""` for a blank `///` line).
pub(crate) fn description_lines(description: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // The open fence's character and length.
    let mut fence: Option<(char, usize)> = None;
    let mut after_list_item: bool = false;
    for line in normalized_lines(description) {
        let line: String = line.replace('\r', " ");
        if let Some((fence_char, length)) = fence {
            let run: usize = line.chars().take_while(|c: &char| *c == fence_char).count();
            if run >= length && line[run * fence_char.len_utf8()..].trim().is_empty() {
                fence = None;
            }
            out.push(visible_controls(&line.replace('\t', "    ")));
            continue;
        }
        if let Some((fence_char, length)) = opening_fence(&line) {
            fence = Some((fence_char, length));
            after_list_item = false;
            out.push(format!("{}text", fence_char.to_string().repeat(length)));
            continue;
        }
        if line.is_empty() {
            after_list_item = false;
            out.push(String::new());
            continue;
        }
        let list_item: bool = is_list_item(&line);
        if after_list_item && !list_item {
            // Not a lazy continuation of the item (`doc_lazy_continuation`): a new paragraph.
            out.push(String::new());
        }
        after_list_item = list_item;
        out.push(render_line(&line));
    }
    if let Some((fence_char, length)) = fence {
        out.push(fence_char.to_string().repeat(length));
    }
    out
}

/// The doc comment lines for an `# Examples` section: each example as compact JSON in a `json`
/// block (fenced with more backticks than any run inside it).
pub(crate) fn examples_lines(examples: &[Value]) -> Vec<String> {
    let texts: Vec<String> = examples
        .iter()
        .map(|example: &Value| visible_controls(&example.to_string()))
        .collect();
    let longest_run: usize = texts
        .iter()
        .flat_map(|text: &String| text.split(|c: char| c != '`'))
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence: String = "`".repeat((longest_run + 1).max(3));
    let mut lines: Vec<String> = vec![
        String::new(),
        "# Examples".to_string(),
        String::new(),
        format!("{fence}json"),
    ];
    lines.extend(texts);
    lines.push(fence);
    lines
}

/// The fence character and length when `line` opens a fenced block.
fn opening_fence(line: &str) -> Option<(char, usize)> {
    let fence_char: char = line
        .chars()
        .next()
        .filter(|c: &char| matches!(c, '`' | '~'))?;
    let length: usize = line.chars().take_while(|c: &char| *c == fence_char).count();
    // A backtick fence's info string cannot hold a backtick (it would be a code span).
    (length >= 3 && !(fence_char == '`' && line[length..].contains('`')))
        .then_some((fence_char, length))
}

/// Whether `line` starts a list item: `-`, `*` or `+`, or digits and `.` or `)`, then a space.
fn is_list_item(line: &str) -> bool {
    let digits: usize = line.chars().take_while(char::is_ascii_digit).count();
    let rest: &str = &line[digits..];
    if digits > 0 {
        rest.starts_with(". ") || rest.starts_with(") ")
    } else {
        ["- ", "* ", "+ "]
            .iter()
            .any(|marker: &&str| line.starts_with(marker))
    }
}

/// One line of text outside a fenced block: code spans as written, everything else rendered.
fn render_line(line: &str) -> String {
    let mut out: String = String::new();
    let mut rest: &str = line;
    while let Some(start) = rest.find('`') {
        render_text(&rest[..start], &mut out);
        let run: usize = rest[start..]
            .chars()
            .take_while(|c: &char| *c == '`')
            .count();
        let after: &str = &rest[start + run..];
        if let Some(end) = closing_run(after, run) {
            out.push_str(&visible_controls(&rest[start..start + run + end + run]));
            rest = &after[end + run..];
        } else {
            // An unmatched run is literal backticks.
            out.push_str(&"\\`".repeat(run));
            rest = after;
        }
    }
    render_text(rest, &mut out);
    out
}

/// The offset in `text` of a run of exactly `run` backticks (a code span's end).
fn closing_run(text: &str, run: usize) -> Option<usize> {
    let bytes: &[u8] = text.as_bytes();
    let mut at: usize = 0;
    while at < bytes.len() {
        if bytes[at] == b'`' {
            let length: usize = bytes[at..].iter().take_while(|b: &&u8| **b == b'`').count();
            if length == run {
                return Some(at);
            }
            at += length;
        } else {
            at += 1;
        }
    }
    None
}

/// Text outside code spans, word by word, split where `doc_markdown` splits it (whitespace and
/// `'`); each separator becomes a space or stays.
fn render_text(text: &str, out: &mut String) {
    let mut word_start: usize = 0;
    for (at, c) in text.char_indices() {
        if c.is_whitespace() || c == '\'' {
            render_word(&text[word_start..at], out);
            out.push(if c == '\'' { '\'' } else { ' ' });
            word_start = at + c.len_utf8();
        }
    }
    render_word(&text[word_start..], out);
}

/// One word: a URL becomes an autolink, an identifier `doc_markdown` flags goes in backticks,
/// and the rest is escaped.
fn render_word(word: &str, out: &mut String) {
    if let Some((start, end)) = url_range(word) {
        escape_markup(&word[..start], out);
        out.push('<');
        out.push_str(&word[start..end]);
        out.push('>');
        escape_markup(&word[end..], out);
        return;
    }
    let (start, end): (usize, usize) = checked_range(word);
    let checked: &str = &word[start..end];
    if needs_backticks(checked) {
        escape_markup(&word[..start], out);
        out.push('`');
        out.push_str(&visible_controls(checked));
        out.push('`');
        escape_markup(&word[end..], out);
    } else {
        escape_markup(word, out);
    }
}

/// The part of `word` that `doc_markdown` checks, as a byte range (clippy's
/// `doc::markdown::check`): trailing then leading characters that are neither alphanumeric nor
/// `:` trimmed, a `()` after it kept, single colons at either end dropped, and closing
/// parentheses kept to balance the opening ones.
fn checked_range(word: &str) -> (usize, usize) {
    let trim = |c: char| !c.is_alphanumeric() && c != ':';
    let mut end: usize = word.trim_end_matches(trim).len();
    if word.get(end..end + 2) == Some("()") {
        end += 2;
    }
    let mut start: usize = end - word[..end].trim_start_matches(trim).len();
    let checked: &str = &word[start..end];
    if checked.starts_with(':') && !checked.starts_with("::") {
        start += checked.len() - checked.trim_start_matches(':').len();
    }
    let checked: &str = &word[start..end];
    if checked.ends_with(':') && !checked.ends_with("::") {
        end -= checked.len() - checked.trim_end_matches(':').len();
    }
    let opened: usize = word[start..end].matches('(').count();
    let mut closed: usize = word[start..end].matches(')').count();
    while closed < opened && word[end..].starts_with(')') {
        end += 1;
        closed += 1;
    }
    (start, end.max(start))
}

/// Whether `doc_markdown` would ask for backticks around `word`, or could under a stricter
/// configuration (an identifier in its `doc-valid-idents` list, or with both `_` and `-`, is
/// wrapped as well: backticks never hurt it).
fn needs_backticks(word: &str) -> bool {
    if word.is_empty() || word.chars().all(|c: char| c == ':') {
        return false;
    }
    (word != "_" && word.contains('_'))
        || word.contains("::")
        || word.ends_with("()")
        || is_camel_case(word)
        // A base URL with a one-letter scheme (`C:/dir`) cannot be an autolink.
        || base_url_scheme(word).is_some()
}

/// Clippy's `is_camel_case`: starts with neither a digit nor a lowercase ASCII letter, and
/// (without a plural ending) is alphanumeric with at least two uppercase letters and one
/// lowercase letter.
fn is_camel_case(word: &str) -> bool {
    if word.starts_with(|c: char| c.is_ascii_digit() || c.is_ascii_lowercase()) {
        return false;
    }
    let all_upper = |s: &str| s.chars().all(|c: char| c.is_ascii_uppercase());
    let stem: &str = if let Some(prefix) = word.strip_suffix("es")
        && all_upper(prefix)
        && matches!(prefix.chars().last(), Some('S' | 'X'))
    {
        prefix
    } else if let Some(prefix) = word.strip_suffix("ified")
        && all_upper(prefix)
    {
        prefix
    } else {
        word.strip_suffix('s').unwrap_or(word)
    };
    stem.chars().all(char::is_alphanumeric)
        && stem.chars().filter(|c: &char| c.is_uppercase()).count() > 1
        && stem.chars().any(char::is_lowercase)
}

/// The length of `word`'s URL scheme (`https` in `https://...`) when it starts with one that
/// makes a base URL and can be an autolink (two or more characters).
fn url_scheme(word: &str) -> Option<usize> {
    base_url_scheme(word).filter(|length: &usize| *length >= 2)
}

/// The length of `word`'s URL scheme when `word` starts with one that makes a base URL
/// (`url::Url::parse` accepts it and `cannot_be_a_base` is false): a scheme, then `:/`, or a
/// special scheme then anything.
fn base_url_scheme(word: &str) -> Option<usize> {
    let scheme_length: usize = word
        .char_indices()
        .take_while(|(i, c): &(usize, char)| {
            c.is_ascii_alphabetic()
                || (*i > 0 && (c.is_ascii_digit() || matches!(c, '+' | '-' | '.')))
        })
        .count();
    let rest: &str = word[scheme_length..].strip_prefix(':')?;
    let special: bool = ["http", "https", "ws", "wss", "ftp", "file"]
        .iter()
        .any(|s: &&str| word[..scheme_length].eq_ignore_ascii_case(s));
    (scheme_length >= 1 && (rest.starts_with('/') || (special && !rest.is_empty())))
        .then_some(scheme_length)
}

/// The byte range of the URL in `word`, when it holds one: from a scheme followed by `://`, or
/// from the start when the whole checked word is a base URL, to the end less trailing
/// punctuation and unbalanced closing brackets.
fn url_range(word: &str) -> Option<(usize, usize)> {
    let start: usize = if let Some(separator) = word.find("://") {
        let scheme: usize = word[..separator]
            .char_indices()
            .rev()
            .take_while(|(_, c): &(usize, char)| {
                c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
            })
            .last()
            .map_or(separator, |(i, _)| i);
        // The scheme starts at its first letter.
        let letter: usize = scheme
            + word[scheme..separator]
                .find(|c: char| c.is_ascii_alphabetic())
                .unwrap_or(separator - scheme);
        (separator - letter >= 2).then_some(letter)?
    } else {
        let (start, end): (usize, usize) = checked_range(word);
        url_scheme(&word[start..end])?;
        start
    };
    let mut end: usize = word.len();
    loop {
        let url: &str = &word[start..end];
        let last: char = url.chars().last()?;
        let unbalanced = |open: char, close: char| {
            last == close && url.matches(open).count() < url.matches(close).count()
        };
        if matches!(
            last,
            '.' | ',' | ';' | ':' | '!' | '?' | '"' | '<' | '>' | '`'
        ) || unbalanced('(', ')')
            || unbalanced('[', ']')
            || unbalanced('{', '}')
        {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    let url: &str = &word[start..end];
    // An autolink holds no `<`, `>` or whitespace, and needs something after the scheme.
    (!url.contains(['<', '>', '`']) && url.contains(':') && !url.ends_with("//"))
        .then_some((start, end))
}

/// `text` with the characters Markdown or rustdoc read as markup escaped: `\` (an escape),
/// `<` and `>` (HTML tags), `[` and `]` (links); control characters made visible.
fn escape_markup(text: &str, out: &mut String) {
    for c in text.chars() {
        if matches!(c, '\\' | '<' | '>' | '[' | ']') {
            out.push('\\');
            out.push(c);
        } else {
            push_visible(c, out);
        }
    }
}

/// `text` with control characters and bidirectional formatting characters (which rustc
/// rejects in comments, `text_direction_codepoint_in_comment`) written as `\u{..}`, and tabs
/// (`tabs_in_doc_comments`) as spaces.
fn visible_controls(text: &str) -> String {
    let mut out: String = String::with_capacity(text.len());
    for c in text.chars() {
        push_visible(c, &mut out);
    }
    out
}

fn push_visible(c: char, out: &mut String) {
    if c == '\t' {
        out.push(' ');
    } else if c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
        let _ = write!(out, "\\u{{{:X}}}", u32::from(c));
    } else {
        out.push(c);
    }
}

#[cfg(test)]
mod tests {
    use super::{description_lines, examples_lines, normalized, read_back};
    use serde_json::json;

    fn lines(description: &str) -> Vec<String> {
        description_lines(description)
    }

    fn owned(lines: &[&str]) -> Vec<String> {
        lines
            .iter()
            .map(|line: &&str| (*line).to_string())
            .collect()
    }

    #[test]
    fn normalized_trims_lines_and_collapses_blank_runs() {
        let expected: Option<String> = Some("a\n\nb".to_string());
        let actual: Option<String> = normalized("\n  a  \n\n\n  b \n\n");
        assert_eq!(expected, actual);
    }

    #[test]
    fn normalized_blank_description_is_none() {
        let expected: Option<String> = None;
        let actual: Option<String> = normalized(" \n \n");
        assert_eq!(expected, actual);
    }

    #[test]
    fn read_back_trims_like_the_derive() {
        let expected: Option<String> = Some("a\n\nb".to_string());
        let actual: Option<String> = read_back(&owned(&["", " a ", "", "b", ""]));
        assert_eq!(expected, actual);
    }

    #[test]
    fn plain_prose_is_unchanged() {
        let expected: Vec<String> = owned(&["The name of the pet, as shown to users."]);
        let actual: Vec<String> = lines("The name of the pet, as shown to users.");
        assert_eq!(expected, actual);
    }

    #[test]
    fn identifiers_doc_markdown_flags_get_backticks() {
        let expected: Vec<String> = owned(&[
            "Set `max_retries` on `WidgetPool` via `Foo::bar` or `reset()`, (`retry_limit`).",
        ]);
        let actual: Vec<String> =
            lines("Set max_retries on WidgetPool via Foo::bar or reset(), (retry_limit).");
        assert_eq!(expected, actual);
    }

    #[test]
    fn words_doc_markdown_accepts_stay_plain() {
        let expected: Vec<String> = owned(&["IDs and NASA and Clippy, _emphasis_ and a_"]);
        let actual: Vec<String> = lines("IDs and NASA and Clippy, _emphasis_ and a_");
        assert_eq!(expected, actual);
    }

    #[test]
    fn apostrophe_splits_words_like_doc_markdown() {
        let expected: Vec<String> = owned(&["The `WidgetPool`'s size."]);
        let actual: Vec<String> = lines("The WidgetPool's size.");
        assert_eq!(expected, actual);
    }

    #[test]
    fn urls_become_autolinks_without_trailing_punctuation() {
        let expected: Vec<String> = owned(&[
            "See <https://example.com/a_b/>, (<http://x.io/p(1)>) and key=<https://y.io>.",
        ]);
        let actual: Vec<String> =
            lines("See https://example.com/a_b/, (http://x.io/p(1)) and key=https://y.io.");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base_url_without_slashes_after_special_scheme_becomes_autolink() {
        let expected: Vec<String> = owned(&["Use <http:host> here"]);
        let actual: Vec<String> = lines("Use http:host here");
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_letter_scheme_path_gets_backticks() {
        let expected: Vec<String> = owned(&["Write to `C:/tmp` now"]);
        let actual: Vec<String> = lines("Write to C:/tmp now");
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_base_urls_stay_plain() {
        let expected: Vec<String> = owned(&["Mail mailto:a@b.c or urn:isbn:1 or ftp:"]);
        let actual: Vec<String> = lines("Mail mailto:a@b.c or urn:isbn:1 or ftp:");
        assert_eq!(expected, actual);
    }

    #[test]
    fn markup_characters_are_escaped() {
        let expected: Vec<String> =
            owned(&["Format \\<host\\>:\\<port\\>, see \\[guide\\] and a\\\\b"]);
        let actual: Vec<String> = lines("Format <host>:<port>, see [guide] and a\\b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn markdown_link_keeps_its_text_and_autolinks_its_url() {
        let expected: Vec<String> = owned(&["\\[docs\\](<https://example.org>)"]);
        let actual: Vec<String> = lines("[docs](https://example.org)");
        assert_eq!(expected, actual);
    }

    #[test]
    fn code_spans_are_kept_as_written() {
        let expected: Vec<String> = owned(&["Use `a_b <c>` or ``x ` y`` then `snake_case`"]);
        let actual: Vec<String> = lines("Use `a_b <c>` or ``x ` y`` then snake_case");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unmatched_backtick_is_escaped() {
        let expected: Vec<String> = owned(&["a \\` b"]);
        let actual: Vec<String> = lines("a ` b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn fences_are_labelled_text_and_their_content_kept() {
        let expected: Vec<String> =
            owned(&["Example:", "```text", "let max_retries = <3>;", "```"]);
        let actual: Vec<String> = lines("Example:\n```rust\nlet max_retries = <3>;\n```");
        assert_eq!(expected, actual);
    }

    #[test]
    fn tilde_fence_closes_only_on_its_own_character() {
        let expected: Vec<String> = owned(&["~~~~text", "```", "~~~~", "after"]);
        let actual: Vec<String> = lines("~~~~\n```\n~~~~\nafter");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unclosed_fence_is_closed() {
        let expected: Vec<String> = owned(&["```text", "code", "```"]);
        let actual: Vec<String> = lines("```\ncode");
        assert_eq!(expected, actual);
    }

    #[test]
    fn line_after_list_item_starts_a_paragraph() {
        let expected: Vec<String> = owned(&["- one", "2. two", "", "after"]);
        let actual: Vec<String> = lines("- one\n2. two\nafter");
        assert_eq!(expected, actual);
    }

    #[test]
    fn tabs_become_spaces() {
        let expected: Vec<String> = owned(&["a b", "```text", "x    y", "```"]);
        let actual: Vec<String> = lines("a\tb\n```\nx\ty\n```");
        assert_eq!(expected, actual);
    }

    #[test]
    fn control_and_bidi_characters_are_made_visible() {
        let expected: Vec<String> = owned(&["a \\u{202E}b\\u{7}"]);
        let actual: Vec<String> = lines("a \u{202E}b\u{7}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn examples_go_in_a_json_block_fenced_past_inner_backticks() {
        let expected: Vec<String> = owned(&[
            "",
            "# Examples",
            "",
            "````json",
            r#"{"note":"use ``` here"}"#,
            "1",
            "````",
        ]);
        let actual: Vec<String> = examples_lines(&[json!({"note": "use ``` here"}), json!(1)]);
        assert_eq!(expected, actual);
    }
}
