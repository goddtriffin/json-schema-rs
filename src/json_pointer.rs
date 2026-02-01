//! JSON Pointer implementation (RFC 6901).
//!
//! Used for identifying a single value within a JSON document.
//! Segments are `/`-separated, with `~` escaped as `~0` and `/` escaped as `~1`.

/// Appends a segment to a JSON Pointer path, applying RFC 6901 escaping.
///
/// Escaping rules: `~` -> `~0`, `/` -> `~1`
pub fn push_segment(path: &mut String, segment: &str) {
    path.push('/');
    for c in segment.chars() {
        match c {
            '~' => path.push_str("~0"),
            '/' => path.push_str("~1"),
            other => path.push(other),
        }
    }
}

/// Returns a new JSON Pointer path by appending a segment to the given path.
///
/// Convenience for building paths without mutating. Applies RFC 6901 escaping.
#[must_use]
pub fn format(path: &str, segment: &str) -> String {
    let mut result: String = path.to_string();
    push_segment(&mut result, segment);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_segment() {
        let mut path = String::new();
        push_segment(&mut path, "foo");
        assert_eq!(path, "/foo");
    }

    #[test]
    fn segment_with_slash() {
        let mut path = String::new();
        push_segment(&mut path, "a/b");
        assert_eq!(path, "/a~1b");
    }

    #[test]
    fn segment_with_tilde() {
        let mut path = String::new();
        push_segment(&mut path, "a~b");
        assert_eq!(path, "/a~0b");
    }

    #[test]
    fn segment_with_both_slash_and_tilde() {
        let mut path = String::new();
        push_segment(&mut path, "~1");
        assert_eq!(path, "/~01");
    }

    #[test]
    fn empty_path_plus_segment() {
        let mut path = String::new();
        push_segment(&mut path, "foo");
        assert_eq!(path, "/foo");
    }

    #[test]
    fn multiple_segments() {
        let mut path = String::new();
        push_segment(&mut path, "properties");
        push_segment(&mut path, "foo");
        push_segment(&mut path, "items");
        assert_eq!(path, "/properties/foo/items");
    }

    #[test]
    fn format_empty_base() {
        assert_eq!(format("", "foo"), "/foo");
    }

    #[test]
    fn format_with_base() {
        assert_eq!(format("/properties", "foo-bar"), "/properties/foo-bar");
    }

    #[test]
    fn format_escapes_slash() {
        assert_eq!(format("", "a/b"), "/a~1b");
    }

    #[test]
    fn format_escapes_tilde() {
        assert_eq!(format("", "a~b"), "/a~0b");
    }

    #[test]
    fn format_empty_segment() {
        assert_eq!(format("/properties", ""), "/properties/");
    }

    #[test]
    fn root_path_empty_segment_produces_slash() {
        // path="" + segment="" -> "/" (single slash: root then empty key)
        assert_eq!(format("", ""), "/");
    }
}
