//! URI references (RFC 3986): splitting, resolution against a base, and normalisation.
//!
//! JSON Schema identifies schema resources by absolute URI (`$id`, `$schema`, retrieval URIs)
//! and points at them with URI references (`$ref`). This module implements exactly what that
//! needs: the generic syntax of RFC 3986 §3 (via the regular expression of Appendix B), reference
//! resolution (§5.2, with dot-segment removal), and the case normalisation of §6.2.2.1 for the
//! scheme. It works on any scheme, including the URNs and `tag:` URIs the specification allows as
//! identifiers, and never percent-decodes anything but a fragment on request.

use std::fmt;

/// The five components of a URI reference (RFC 3986 §3), borrowed from the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Components<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

impl<'a> Components<'a> {
    /// Splits `text` the way RFC 3986 Appendix B's regular expression does. Every string is a URI
    /// reference under this split, so it cannot fail.
    fn split(text: &'a str) -> Self {
        let (rest, fragment): (&str, Option<&str>) = match text.split_once('#') {
            Some((rest, fragment)) => (rest, Some(fragment)),
            None => (text, None),
        };
        let (rest, query): (&str, Option<&str>) = match rest.split_once('?') {
            Some((rest, query)) => (rest, Some(query)),
            None => (rest, None),
        };
        // A scheme is the text before the first ':' when no '/' comes before it, and it starts
        // with a letter (§3.1); anything else is a relative reference.
        let (scheme, rest): (Option<&str>, &str) = match rest.find(':') {
            Some(i) if is_scheme(&rest[..i]) => (Some(&rest[..i]), &rest[i + 1..]),
            _ => (None, rest),
        };
        let (authority, path): (Option<&str>, &str) = match rest.strip_prefix("//") {
            Some(after) => {
                let end: usize = after.find('/').unwrap_or(after.len());
                (Some(&after[..end]), &after[end..])
            }
            None => (None, rest),
        };
        Components {
            scheme,
            authority,
            path,
            query,
            fragment,
        }
    }

    /// Recomposes the components (RFC 3986 §5.3), lower-casing the scheme (§6.2.2.1).
    fn recompose(&self) -> String {
        let mut out: String = String::new();
        if let Some(scheme) = self.scheme {
            out.push_str(&scheme.to_ascii_lowercase());
            out.push(':');
        }
        if let Some(authority) = self.authority {
            out.push_str("//");
            out.push_str(authority);
        }
        out.push_str(self.path);
        if let Some(query) = self.query {
            out.push('?');
            out.push_str(query);
        }
        if let Some(fragment) = self.fragment {
            out.push('#');
            out.push_str(fragment);
        }
        out
    }
}

/// Whether `s` is a scheme: a letter followed by letters, digits, `+`, `-` or `.` (§3.1).
fn is_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Removes `.` and `..` segments from a path (RFC 3986 §5.2.4), iteratively.
fn remove_dot_segments(path: &str) -> String {
    let mut input: &str = path;
    let mut output: Vec<&str> = Vec::new();
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix("../") {
            input = rest;
        } else if let Some(rest) = input.strip_prefix("./") {
            input = rest;
        } else if input.starts_with("/./") {
            input = &input[2..];
        } else if input == "/." {
            input = "/";
        } else if input.starts_with("/../") {
            input = &input[3..];
            output.pop();
        } else if input == "/.." {
            input = "/";
            output.pop();
        } else if input == "." || input == ".." {
            input = "";
        } else {
            // Move the first segment (with its leading '/', if any) to the output.
            let start: usize = usize::from(input.starts_with('/'));
            let end: usize = input[start..].find('/').map_or(input.len(), |i| i + start);
            output.push(&input[..end]);
            input = &input[end..];
        }
    }
    output.concat()
}

/// Merges a relative path with the base's path (RFC 3986 §5.2.3).
fn merge_paths(base: &Components<'_>, reference_path: &str) -> String {
    if base.authority.is_some() && base.path.is_empty() {
        return format!("/{reference_path}");
    }
    match base.path.rfind('/') {
        Some(i) => format!("{}{reference_path}", &base.path[..=i]),
        None => reference_path.to_string(),
    }
}

/// An absolute URI: it has a scheme, and its scheme is lower case. It may carry a fragment
/// (`$ref` targets do; resource identifiers do not, see [`Uri::without_fragment`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uri(String);

/// A string that is not an absolute URI where one is required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAbsoluteUri {
    /// The string as given.
    pub value: String,
}

impl fmt::Display for NotAbsoluteUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\"{}\" is not an absolute URI (it has no scheme)",
            self.value
        )
    }
}

impl std::error::Error for NotAbsoluteUri {}

impl Uri {
    /// Parses an absolute URI, removing dot segments from its path and lower-casing its scheme.
    ///
    /// # Errors
    ///
    /// Returns [`NotAbsoluteUri`] when `text` has no scheme.
    pub fn parse(text: &str) -> Result<Uri, NotAbsoluteUri> {
        let components: Components<'_> = Components::split(text);
        if components.scheme.is_none() {
            return Err(NotAbsoluteUri {
                value: text.to_string(),
            });
        }
        let path: String = remove_dot_segments(components.path);
        Ok(Uri(Components {
            path: &path,
            ..components
        }
        .recompose()))
    }

    /// Resolves the URI reference `reference` against this URI as its base (RFC 3986 §5.2.2).
    /// The result keeps the reference's fragment, if any.
    #[must_use]
    pub fn resolve(&self, reference: &str) -> Uri {
        let base: Components<'_> = Components::split(&self.0);
        let r: Components<'_> = Components::split(reference);
        let path: String;
        let target: Components<'_> = if r.scheme.is_some() {
            path = remove_dot_segments(r.path);
            Components { path: &path, ..r }
        } else if r.authority.is_some() {
            path = remove_dot_segments(r.path);
            Components {
                scheme: base.scheme,
                path: &path,
                ..r
            }
        } else if r.path.is_empty() {
            Components {
                scheme: base.scheme,
                authority: base.authority,
                path: base.path,
                query: r.query.or(base.query),
                fragment: r.fragment,
            }
        } else {
            path = if r.path.starts_with('/') {
                remove_dot_segments(r.path)
            } else {
                remove_dot_segments(&merge_paths(&base, r.path))
            };
            Components {
                scheme: base.scheme,
                authority: base.authority,
                path: &path,
                query: r.query,
                fragment: r.fragment,
            }
        };
        Uri(target.recompose())
    }

    /// This URI with its fragment (and the `#` before it) removed.
    #[must_use]
    pub fn without_fragment(&self) -> Uri {
        match self.0.split_once('#') {
            Some((rest, _)) => Uri(rest.to_string()),
            None => self.clone(),
        }
    }

    /// The fragment as written (still percent-encoded), or `None` when there is no `#`.
    #[must_use]
    pub fn fragment(&self) -> Option<&str> {
        self.0.split_once('#').map(|(_, fragment)| fragment)
    }

    /// The URI as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Splits a URI reference into the part before `#` and the fragment after it (`None` when there
/// is no `#`).
#[must_use]
pub fn split_fragment(reference: &str) -> (&str, Option<&str>) {
    match reference.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (reference, None),
    }
}

/// Decodes `%XX` escapes in a fragment (RFC 3986 §2.1). Returns `None` when an escape is
/// malformed or the decoded bytes are not UTF-8.
#[must_use]
pub fn percent_decode(text: &str) -> Option<String> {
    if !text.contains('%') {
        return Some(text.to_string());
    }
    let bytes: &[u8] = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i: usize = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex: &str = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::{NotAbsoluteUri, Uri, percent_decode, split_fragment};

    fn resolve(base: &str, reference: &str) -> String {
        Uri::parse(base).unwrap().resolve(reference).to_string()
    }

    // RFC 3986 §5.4.1 normal examples, base "http://a/b/c/d;p?q".
    #[test]
    fn resolve_rfc3986_normal_examples() {
        let references: [&str; 23] = [
            "g:h", "g", "./g", "g/", "/g", "//g", "?y", "g?y", "#s", "g#s", "g?y#s", ";x", "g;x",
            "g;x?y#s", "", ".", "./", "..", "../", "../g", "../..", "../../", "../../g",
        ];
        let expected: Vec<&str> = vec![
            "g:h",
            "http://a/b/c/g",
            "http://a/b/c/g",
            "http://a/b/c/g/",
            "http://a/g",
            "http://g",
            "http://a/b/c/d;p?y",
            "http://a/b/c/g?y",
            "http://a/b/c/d;p?q#s",
            "http://a/b/c/g#s",
            "http://a/b/c/g?y#s",
            "http://a/b/c/;x",
            "http://a/b/c/g;x",
            "http://a/b/c/g;x?y#s",
            "http://a/b/c/d;p?q",
            "http://a/b/c/",
            "http://a/b/c/",
            "http://a/b/",
            "http://a/b/",
            "http://a/b/g",
            "http://a/",
            "http://a/",
            "http://a/g",
        ];
        let actual: Vec<String> = references
            .iter()
            .map(|r| resolve("http://a/b/c/d;p?q", r))
            .collect();
        assert_eq!(expected, actual);
    }

    // RFC 3986 §5.4.2 abnormal examples.
    #[test]
    fn resolve_rfc3986_abnormal_examples() {
        let references: [&str; 8] = [
            "../../../g",
            "/./g",
            "/../g",
            "g.",
            "g..",
            "./../g",
            "g/./h",
            "g/../h",
        ];
        let expected: Vec<&str> = vec![
            "http://a/g",
            "http://a/g",
            "http://a/g",
            "http://a/b/c/g.",
            "http://a/b/c/g..",
            "http://a/b/g",
            "http://a/b/c/g/h",
            "http://a/b/c/h",
        ];
        let actual: Vec<String> = references
            .iter()
            .map(|r| resolve("http://a/b/c/d;p?q", r))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_fragment_against_urn() {
        let expected: &str = "urn:uuid:deadbeef-1234-ffff-ffff-4321feebdaed#/$defs/bar";
        let actual: String = resolve(
            "urn:uuid:deadbeef-1234-ffff-ffff-4321feebdaed",
            "#/$defs/bar",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_absolute_path_against_default_base() {
        let expected: &str = "json-schema:///base";
        let actual: String = resolve("json-schema:///", "/base");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_lowercases_scheme_and_removes_dot_segments() {
        let expected: &str = "http://Example.com/b/c";
        let actual: String = Uri::parse("HTTP://Example.com/a/../b/./c")
            .unwrap()
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_rejects_relative_reference() {
        let expected: NotAbsoluteUri = NotAbsoluteUri {
            value: "folder/schema.json".to_string(),
        };
        let actual: NotAbsoluteUri = Uri::parse("folder/schema.json").unwrap_err();
        assert_eq!(expected, actual);
    }

    #[test]
    fn without_fragment_drops_hash() {
        let expected: &str = "http://example.com/root.json";
        let actual: String = Uri::parse("http://example.com/root.json#")
            .unwrap()
            .without_fragment()
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn fragment_returns_text_after_hash() {
        let expected: Option<&str> = Some("/$defs/a");
        let uri: Uri = Uri::parse("http://example.com/root.json#/$defs/a").unwrap();
        let actual: Option<&str> = uri.fragment();
        assert_eq!(expected, actual);
    }

    #[test]
    fn split_fragment_without_hash() {
        let expected: (&str, Option<&str>) = ("a.json", None);
        let actual: (&str, Option<&str>) = split_fragment("a.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn percent_decode_quote() {
        let expected: Option<String> = Some("/$defs/foo\"bar".to_string());
        let actual: Option<String> = percent_decode("/$defs/foo%22bar");
        assert_eq!(expected, actual);
    }

    #[test]
    fn percent_decode_rejects_malformed_escape() {
        let expected: Option<String> = None;
        let actual: Option<String> = percent_decode("/a%2");
        assert_eq!(expected, actual);
    }
}
