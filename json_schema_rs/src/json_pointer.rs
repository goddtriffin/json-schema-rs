//! JSON Pointer (RFC 6901): type and helpers for building and parsing pointer strings.
//!
//! A pointer is either the empty string (whole document) or a sequence of
//! reference tokens separated by `/`. In each token, `~0` represents `~` and
//! `~1` represents `/`. Segments are stored decoded; encoding is applied when
//! producing the pointer string.

use std::fmt;

/// Error when parsing a string as a JSON Pointer (RFC 6901).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonPointerError {
    /// Invalid escape: `~` not followed by `0` or `1`.
    InvalidEscape,
    /// Input is not valid UTF-8.
    InvalidUtf8,
}

impl fmt::Display for JsonPointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonPointerError::InvalidEscape => {
                write!(
                    f,
                    "invalid JSON Pointer escape: ~ must be followed by 0 or 1"
                )
            }
            JsonPointerError::InvalidUtf8 => write!(f, "JSON Pointer is not valid UTF-8"),
        }
    }
}

impl std::error::Error for JsonPointerError {}

/// Encode one segment for RFC 6901: `~` → `~0`, `/` → `~1`.
fn encode_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// Decode one reference token: first `~1` → `/`, then `~0` → `~`.
fn decode_token(token: &str) -> Result<String, JsonPointerError> {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '~' {
            let next = chars.next().ok_or(JsonPointerError::InvalidEscape)?;
            match next {
                '0' => out.push('~'),
                '1' => out.push('/'),
                _ => return Err(JsonPointerError::InvalidEscape),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// A JSON Pointer (RFC 6901): identifies a value within a JSON document.
///
/// Stored as decoded segments; the pointer string is produced by encoding
/// when needed. Root has zero segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonPointer {
    /// Decoded reference tokens; empty = root.
    segments: Vec<String>,
    /// Cached RFC 6901 string ("" or "/seg1/seg2/...") for `as_str()` and Display.
    encoded: String,
}

impl JsonPointer {
    /// Root pointer (whole document). Equal to the empty string.
    #[must_use]
    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
            encoded: String::new(),
        }
    }

    fn from_segments_and_encoded(segments: Vec<String>, encoded: String) -> Self {
        Self { segments, encoded }
    }

    /// Returns a new pointer with one more segment. The segment is stored
    /// decoded; encoding is applied when producing the pointer string.
    #[must_use]
    pub fn push(&self, segment: &str) -> Self {
        let mut new_segments = self.segments.clone();
        new_segments.push(segment.to_string());
        let encoded = if new_segments.is_empty() {
            String::new()
        } else {
            format!(
                "/{}",
                new_segments
                    .iter()
                    .map(String::as_str)
                    .map(encode_segment)
                    .collect::<Vec<_>>()
                    .join("/")
            )
        };
        Self {
            segments: new_segments,
            encoded,
        }
    }

    /// Returns a new pointer with the last segment removed. Root unchanged.
    #[must_use]
    pub fn pop(&self) -> Self {
        if self.segments.is_empty() {
            return self.clone();
        }
        let mut segs = self.segments.clone();
        segs.pop();
        let enc = if segs.is_empty() {
            String::new()
        } else {
            format!(
                "/{}",
                segs.iter()
                    .map(String::as_str)
                    .map(encode_segment)
                    .collect::<Vec<_>>()
                    .join("/")
            )
        };
        Self {
            segments: segs,
            encoded: enc,
        }
    }

    /// Returns the parent pointer (same as popping the last segment).
    #[must_use]
    pub fn parent(&self) -> Self {
        self.pop()
    }

    /// Returns a new pointer with only the first `len` segments.
    #[must_use]
    pub fn truncate(&self, len: usize) -> Self {
        if len >= self.segments.len() {
            return self.clone();
        }
        let segs: Vec<String> = self.segments[..len].to_vec();
        let enc = if segs.is_empty() {
            String::new()
        } else {
            format!(
                "/{}",
                segs.iter()
                    .map(String::as_str)
                    .map(encode_segment)
                    .collect::<Vec<_>>()
                    .join("/")
            )
        };
        Self {
            segments: segs,
            encoded: enc,
        }
    }

    /// Number of segments (0 for root).
    #[must_use]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Returns true if this pointer has no segments (root).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns true if this is the root pointer.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns an iterator over the decoded segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.segments.iter().map(String::as_str)
    }

    /// Returns the segment at `index`, or `None` if out of bounds.
    #[must_use]
    pub fn segment_at(&self, index: usize) -> Option<&str> {
        self.segments.get(index).map(String::as_str)
    }

    /// Returns a new pointer with the segment at `index` removed.
    #[must_use]
    pub fn remove(&self, index: usize) -> Self {
        if index >= self.segments.len() {
            return self.clone();
        }
        let mut segs = self.segments.clone();
        segs.remove(index);
        let enc = if segs.is_empty() {
            String::new()
        } else {
            format!(
                "/{}",
                segs.iter()
                    .map(String::as_str)
                    .map(encode_segment)
                    .collect::<Vec<_>>()
                    .join("/")
            )
        };
        Self {
            segments: segs,
            encoded: enc,
        }
    }

    /// Returns the pointer as a string ("" for root, "/a/b" for children).
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.encoded.as_str()
    }

    /// Returns a display-friendly location: "root" when empty, otherwise the pointer string.
    #[must_use]
    pub fn display_root_or_path(&self) -> &str {
        if self.encoded.is_empty() {
            "root"
        } else {
            self.encoded.as_str()
        }
    }
}

impl fmt::Display for JsonPointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encoded)
    }
}

/// Parse a non-empty pointer: must start with `/`; split by `/`, decode each token.
fn try_parse(s: &str) -> Result<JsonPointer, JsonPointerError> {
    if s.is_empty() {
        return Ok(JsonPointer::root());
    }
    if !s.starts_with('/') {
        return Err(JsonPointerError::InvalidEscape);
    }
    let parts: Vec<&str> = s.split('/').collect();
    let mut segments = Vec::with_capacity(parts.len().saturating_sub(1));
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            continue;
        }
        segments.push(decode_token(part)?);
    }
    let encoded = s.to_string();
    Ok(JsonPointer::from_segments_and_encoded(segments, encoded))
}

impl TryFrom<&str> for JsonPointer {
    type Error = JsonPointerError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        try_parse(s)
    }
}

impl TryFrom<String> for JsonPointer {
    type Error = JsonPointerError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        try_parse(&s)
    }
}

impl TryFrom<&[u8]> for JsonPointer {
    type Error = JsonPointerError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(bytes).map_err(|_| JsonPointerError::InvalidUtf8)?;
        try_parse(s)
    }
}

impl TryFrom<Vec<u8>> for JsonPointer {
    type Error = JsonPointerError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let s = String::from_utf8(bytes).map_err(|_| JsonPointerError::InvalidUtf8)?;
        try_parse(&s)
    }
}

impl From<Vec<String>> for JsonPointer {
    fn from(segments: Vec<String>) -> Self {
        let encoded = if segments.is_empty() {
            String::new()
        } else {
            format!(
                "/{}",
                segments
                    .iter()
                    .map(String::as_str)
                    .map(encode_segment)
                    .collect::<Vec<_>>()
                    .join("/")
            )
        };
        Self { segments, encoded }
    }
}

impl From<JsonPointer> for String {
    fn from(p: JsonPointer) -> Self {
        p.encoded
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonPointer, JsonPointerError};
    use std::convert::TryFrom;

    #[test]
    fn root_is_empty() {
        let p: JsonPointer = JsonPointer::root();
        let expected = "";
        let actual = p.as_str();
        assert_eq!(expected, actual);
        assert_eq!(p.to_string(), "");
        assert!(p.is_root());
        assert_eq!(p.len(), 0);
    }

    #[test]
    fn push_one_segment() {
        let p: JsonPointer = JsonPointer::root().push("foo");
        let expected = "/foo";
        let actual = p.as_str();
        assert_eq!(expected, actual);
    }

    #[test]
    fn push_multiple_segments() {
        let p: JsonPointer = JsonPointer::root().push("a").push("b").push("c");
        let expected = "/a/b/c";
        let actual = p.as_str();
        assert_eq!(expected, actual);
    }

    #[test]
    fn segment_with_slash_encoded_as_tilde1() {
        let p: JsonPointer = JsonPointer::root().push("a/b");
        let expected = "/a~1b";
        let actual = p.as_str();
        assert_eq!(expected, actual);
    }

    #[test]
    fn segment_with_tilde_encoded_as_tilde0() {
        let p: JsonPointer = JsonPointer::root().push("a~b");
        let expected = "/a~0b";
        let actual = p.as_str();
        assert_eq!(expected, actual);
    }

    #[test]
    fn segment_with_both_tilde_and_slash() {
        let p: JsonPointer = JsonPointer::root().push("~1").push("a/b");
        let expected = "/~01/a~1b";
        let actual = p.as_str();
        assert_eq!(expected, actual);
    }

    #[test]
    fn display_root_or_path() {
        let root: JsonPointer = JsonPointer::root();
        let expected = "root";
        let actual = root.display_root_or_path();
        assert_eq!(expected, actual);
        let child: JsonPointer = JsonPointer::root().push("x");
        let expected_path = "/x";
        let actual_path = child.display_root_or_path();
        assert_eq!(expected_path, actual_path);
    }

    #[test]
    fn pop_from_one_segment_gives_root() {
        let p: JsonPointer = JsonPointer::root().push("a");
        let expected = JsonPointer::root();
        let actual = p.pop();
        assert_eq!(expected.as_str(), actual.as_str());
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn pop_from_three_segments_gives_two() {
        let p: JsonPointer = JsonPointer::root().push("a").push("b").push("c");
        let expected = JsonPointer::root().push("a").push("b");
        let actual = p.pop();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn pop_from_root_leaves_root() {
        let root: JsonPointer = JsonPointer::root();
        let actual = root.pop();
        assert_eq!(root.as_str(), actual.as_str());
    }

    #[test]
    fn parent_matches_pop() {
        let p: JsonPointer = JsonPointer::root().push("x").push("y");
        let expected = p.pop();
        let actual = p.parent();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn truncate_to_zero_gives_root() {
        let p: JsonPointer = JsonPointer::root().push("a").push("b");
        let actual = p.truncate(0);
        assert_eq!(JsonPointer::root().as_str(), actual.as_str());
    }

    #[test]
    fn truncate_to_one() {
        let p: JsonPointer = JsonPointer::root().push("a").push("b").push("c");
        let expected = JsonPointer::root().push("a");
        let actual = p.truncate(1);
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn truncate_to_current_len_unchanged() {
        let p: JsonPointer = JsonPointer::root().push("a").push("b");
        let actual = p.truncate(2);
        assert_eq!(p.as_str(), actual.as_str());
    }

    #[test]
    fn truncate_beyond_len_unchanged() {
        let p: JsonPointer = JsonPointer::root().push("a");
        let actual = p.truncate(10);
        assert_eq!(p.as_str(), actual.as_str());
    }

    #[test]
    fn segments_iterator() {
        let p: JsonPointer = JsonPointer::root().push("a").push("b").push("c");
        let expected: Vec<&str> = vec!["a", "b", "c"];
        let actual: Vec<&str> = p.segments().collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn segment_at_valid() {
        let p: JsonPointer = JsonPointer::root().push("x").push("y");
        let expected_first = Some("x");
        let actual_first = p.segment_at(0);
        assert_eq!(expected_first, actual_first);
        let expected_second = Some("y");
        let actual_second = p.segment_at(1);
        assert_eq!(expected_second, actual_second);
    }

    #[test]
    fn segment_at_out_of_bounds() {
        let p: JsonPointer = JsonPointer::root().push("a");
        let expected: Option<&str> = None;
        let actual = p.segment_at(1);
        assert_eq!(expected, actual);
    }

    #[test]
    fn remove_segment() {
        let p: JsonPointer = JsonPointer::root().push("a").push("b").push("c");
        let actual = p.remove(1);
        let expected = JsonPointer::root().push("a").push("c");
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_empty_string() {
        let expected = JsonPointer::root();
        let actual = JsonPointer::try_from("").unwrap();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_slash_a() {
        let expected = JsonPointer::root().push("a");
        let actual = JsonPointer::try_from("/a").unwrap();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_slash_a_slash_b() {
        let expected = JsonPointer::root().push("a").push("b");
        let actual = JsonPointer::try_from("/a/b").unwrap();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_encoded_slash() {
        let expected = JsonPointer::root().push("a/b");
        let actual = JsonPointer::try_from("/a~1b").unwrap();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_encoded_tilde() {
        let expected = JsonPointer::root().push("a~b");
        let actual = JsonPointer::try_from("/a~0b").unwrap();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_encoded_tilde1_segment() {
        let expected = JsonPointer::root().push("~1");
        let actual = JsonPointer::try_from("/~01").unwrap();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_invalid_escape_tilde_only() {
        let actual = JsonPointer::try_from("~");
        assert!(matches!(actual, Err(JsonPointerError::InvalidEscape)));
    }

    #[test]
    fn try_from_invalid_escape_tilde_two() {
        let actual = JsonPointer::try_from("/a~2b");
        assert!(matches!(actual, Err(JsonPointerError::InvalidEscape)));
    }

    #[test]
    fn try_from_no_leading_slash_rejected() {
        let actual = JsonPointer::try_from("a");
        assert!(matches!(actual, Err(JsonPointerError::InvalidEscape)));
    }

    #[test]
    fn from_vec_string_empty() {
        let expected = JsonPointer::root();
        let actual: JsonPointer = Vec::<String>::new().into();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn from_vec_string_two() {
        let expected = JsonPointer::root().push("a").push("b");
        let actual: JsonPointer = vec!["a".to_string(), "b".to_string()].into();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_bytes_valid_utf8() {
        let expected = JsonPointer::root().push("x");
        let actual = JsonPointer::try_from("/x".as_bytes()).unwrap();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn try_from_bytes_invalid_utf8() {
        let actual = JsonPointer::try_from(vec![0xff, 0xfe].as_slice());
        assert!(matches!(actual, Err(JsonPointerError::InvalidUtf8)));
    }

    #[test]
    fn into_string() {
        let p: JsonPointer = JsonPointer::root().push("foo").push("bar");
        let expected = "/foo/bar";
        let actual: String = p.into();
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_build_serialize_parse() {
        let expected = JsonPointer::root().push("a").push("b").push("c");
        let s = expected.to_string();
        let actual = JsonPointer::try_from(s.as_str()).unwrap();
        assert_eq!(expected.as_str(), actual.as_str());
    }

    #[test]
    fn round_trip_parse_serialize_parse() {
        let s = "/a~1b/c~0d";
        let p = JsonPointer::try_from(s).unwrap();
        let s2 = p.to_string();
        let p2 = JsonPointer::try_from(s2.as_str()).unwrap();
        assert_eq!(p.as_str(), p2.as_str());
    }

    #[test]
    fn empty_segment() {
        let p: JsonPointer = JsonPointer::root().push("");
        let expected = "/";
        let actual = p.as_str();
        assert_eq!(expected, actual);
    }

    #[test]
    fn len_after_push() {
        let root = JsonPointer::root();
        assert_eq!(root.len(), 0);
        let one = root.push("a");
        assert_eq!(one.len(), 1);
        let three = one.push("b").push("c");
        assert_eq!(three.len(), 3);
    }
}
