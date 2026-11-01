//! JSON Pointer (RFC 6901): type and helpers for building and parsing pointer strings.
//!
//! A pointer is either the empty string (whole document) or a sequence of
//! reference tokens separated by `/`. In each token, `~0` represents `~` and
//! `~1` represents `/`. A pointer keeps its encoded string, and its decoded segments
//! next to it only when some segment needed escaping.

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

/// Whether `segment` holds a character RFC 6901 escapes (`~`, `/`).
fn needs_escape(segment: &str) -> bool {
    segment.bytes().any(|b| b == b'~' || b == b'/')
}

/// Appends one segment to `out`, encoded for RFC 6901: `~` → `~0`, `/` → `~1`.
fn encode_segment(segment: &str, out: &mut String) {
    if !needs_escape(segment) {
        out.push_str(segment);
        return;
    }
    for c in segment.chars() {
        match c {
            '~' => out.push_str("~0"),
            '/' => out.push_str("~1"),
            c => out.push(c),
        }
    }
}

/// Appends `segments` as a JSON Pointer in a URI fragment (RFC 6901 §6): each segment escaped
/// for the pointer (`~0`, `~1`), then every byte a fragment may not hold as written
/// percent-encoded (RFC 3986 §3.5: unreserved characters, sub-delimiters, `:`, `@`, `/` and `?`
/// stay; non-ASCII characters are kept, as an IRI allows).
pub(crate) fn push_uri_fragment<'s>(segments: impl Iterator<Item = &'s str>, out: &mut String) {
    let keep = |c: char| -> bool {
        !c.is_ascii()
            || c.is_ascii_alphanumeric()
            || matches!(
                c,
                '-' | '.'
                    | '_'
                    | '~'
                    | '!'
                    | '$'
                    | '&'
                    | '\''
                    | '('
                    | ')'
                    | '*'
                    | '+'
                    | ','
                    | ';'
                    | '='
                    | ':'
                    | '@'
                    | '/'
                    | '?'
            )
    };
    let mut escaped: String = String::new();
    for segment in segments {
        out.push('/');
        // Most segments need no escape of either kind: copied whole.
        if !needs_escape(segment) && segment.chars().all(keep) {
            out.push_str(segment);
            continue;
        }
        escaped.clear();
        encode_segment(segment, &mut escaped);
        for c in escaped.chars() {
            if keep(c) {
                out.push(c);
            } else {
                // ASCII: one byte, two hex digits.
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                let byte: usize = c as usize;
                out.push('%');
                out.push(char::from(HEX[byte >> 4]));
                out.push(char::from(HEX[byte & 0xF]));
            }
        }
    }
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
/// Stored as one buffer, the encoded pointer string followed (only when some segment needs
/// escaping, `~` or `/`) by the decoded segments one after another, and where each segment
/// ends. Two allocations whatever the depth. Root has zero segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonPointer {
    /// The RFC 6901 string ("" or "/seg1/seg2/..."), then the decoded segments when they differ
    /// from the encoded ones.
    buffer: String,
    /// The length of the RFC 6901 string at the start of `buffer`.
    encoded_len: usize,
    /// Where each segment ends in `buffer`: in the decoded part when there is one, else in the
    /// encoded string (each segment there starts one byte after the previous end, past its
    /// `/`). Empty = root.
    ends: Box<[usize]>,
}

impl JsonPointer {
    /// Root pointer (whole document). Equal to the empty string.
    #[must_use]
    pub fn root() -> Self {
        Self {
            buffer: String::new(),
            encoded_len: 0,
            ends: Box::new([]),
        }
    }

    /// Whether the decoded segments are kept apart, after the encoded string.
    fn has_decoded(&self) -> bool {
        // An escaped segment holds `~` or `/`, so the decoded part is never empty.
        self.buffer.len() > self.encoded_len
    }

    /// The pointer of `segments` (decoded).
    pub(crate) fn from_segments<'s, I>(segments: I) -> Self
    where
        I: IntoIterator<Item = &'s str>,
        I::IntoIter: Clone,
    {
        let segments = segments.into_iter();
        let (count, bytes, escaped): (usize, usize, bool) =
            segments
                .clone()
                .fold((0, 0, false), |(count, bytes, escaped), segment| {
                    (
                        count + 1,
                        bytes + segment.len(),
                        escaped || needs_escape(segment),
                    )
                });
        let mut buffer: String = String::with_capacity(if escaped {
            2 * bytes + 2 * count
        } else {
            bytes + count
        });
        let mut ends: Vec<usize> = Vec::with_capacity(count);
        for segment in segments.clone() {
            buffer.push('/');
            encode_segment(segment, &mut buffer);
            if !escaped {
                ends.push(buffer.len());
            }
        }
        let encoded_len: usize = buffer.len();
        if escaped {
            for segment in segments {
                buffer.push_str(segment);
                ends.push(buffer.len());
            }
        }
        Self {
            buffer,
            encoded_len,
            ends: ends.into_boxed_slice(),
        }
    }

    /// Appends `segment` (decoded) in place.
    fn push_segment(&mut self, segment: &str) {
        if !self.has_decoded() && !needs_escape(segment) {
            self.buffer.push('/');
            encode_segment(segment, &mut self.buffer);
            self.encoded_len = self.buffer.len();
            let mut ends: Vec<usize> = std::mem::take(&mut self.ends).into_vec();
            ends.push(self.encoded_len);
            self.ends = ends.into_boxed_slice();
            return;
        }
        let segments: Vec<&str> = self.segments().chain(std::iter::once(segment)).collect();
        *self = Self::from_segments(segments);
    }

    /// Returns a new pointer with one more segment. The segment is stored
    /// decoded; encoding is applied when producing the pointer string.
    #[must_use]
    pub fn push(&self, segment: &str) -> Self {
        let mut pointer: JsonPointer = self.clone();
        pointer.push_segment(segment);
        pointer
    }

    /// Returns a new pointer with the last segment removed. Root unchanged.
    #[must_use]
    pub fn pop(&self) -> Self {
        self.truncate(self.len().saturating_sub(1))
    }

    /// Returns a new pointer with only the first `len` segments.
    #[must_use]
    pub fn truncate(&self, len: usize) -> Self {
        if len >= self.len() {
            return self.clone();
        }
        let segments: Vec<&str> = self.segments().take(len).collect();
        Self::from_segments(segments)
    }

    /// Number of segments (0 for root).
    #[must_use]
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    /// Returns true if this pointer has no segments (root).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Returns an iterator over the decoded segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> + Clone {
        (0..self.ends.len()).map(|i| self.segment(i))
    }

    /// The segment at `index`, which is in bounds.
    fn segment(&self, index: usize) -> &str {
        if self.has_decoded() {
            let start: usize = if index == 0 {
                self.encoded_len
            } else {
                self.ends[index - 1]
            };
            &self.buffer[start..self.ends[index]]
        } else {
            let previous: usize = if index == 0 { 0 } else { self.ends[index - 1] };
            // Past the segment's `/`.
            &self.buffer[previous + 1..self.ends[index]]
        }
    }

    /// Returns the segment at `index`, or `None` if out of bounds.
    #[must_use]
    pub fn segment_at(&self, index: usize) -> Option<&str> {
        (index < self.len()).then(|| self.segment(index))
    }

    /// Returns a new pointer with the segment at `index` removed.
    #[must_use]
    pub fn remove(&self, index: usize) -> Self {
        if index >= self.len() {
            return self.clone();
        }
        let segments: Vec<&str> = self
            .segments()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, segment)| segment)
            .collect();
        Self::from_segments(segments)
    }

    /// Returns the pointer as a string ("" for root, "/a/b" for children).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.buffer[..self.encoded_len]
    }

    /// Returns a display-friendly location: "root" when empty, otherwise the pointer string.
    #[must_use]
    pub fn display_root_or_path(&self) -> &str {
        if self.encoded_len == 0 {
            "root"
        } else {
            self.as_str()
        }
    }
}

impl fmt::Display for JsonPointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    let segments: Vec<String> = s
        .split('/')
        .skip(1)
        .map(decode_token)
        .collect::<Result<_, _>>()?;
    Ok(JsonPointer::from_segments(
        segments.iter().map(String::as_str),
    ))
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
        Self::from_segments(segments.iter().map(String::as_str))
    }
}

impl From<JsonPointer> for String {
    fn from(mut p: JsonPointer) -> Self {
        p.buffer.truncate(p.encoded_len);
        p.buffer
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

    #[test]
    fn from_segments_encodes_tilde_and_slash() {
        let pointer: JsonPointer = JsonPointer::from(vec![
            "a~b".to_string(),
            "c/d".to_string(),
            "~/".to_string(),
            "é".to_string(),
        ]);
        let expected: &str = "/a~0b/c~1d/~0~1/é";
        let actual: &str = pointer.as_str();
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_no_segments_is_root() {
        let expected: JsonPointer = JsonPointer::root();
        let actual: JsonPointer = JsonPointer::from(Vec::<String>::new());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_empty_segment_keeps_its_slash() {
        let pointer: JsonPointer = JsonPointer::from(vec![String::new(), "a".to_string()]);
        let expected: &str = "//a";
        let actual: &str = pointer.as_str();
        assert_eq!(expected, actual);
    }
}
