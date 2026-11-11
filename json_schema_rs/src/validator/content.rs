//! The content keywords' checks: `contentEncoding` and `contentMediaType` where the dialect
//! asserts them ([`ContentMode::Assertion`](crate::ContentMode::Assertion)).
//!
//! Only what can be decided without a codec registry is checked: `base64` (RFC 4648, section 4)
//! and JSON media types (`application/json` and any `+json` type, RFC 6839). Any other encoding
//! or media type stays an annotation, and so does a media type under an encoding this library
//! cannot decode.

use std::borrow::Cow;

/// An encoding `contentEncoding` names that this library decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentEncoding {
    /// RFC 4648, section 4: the standard alphabet, padded to a multiple of four characters.
    Base64,
}

impl ContentEncoding {
    /// The encoding `name` names, or `None` for one this library does not decode. Names are
    /// case-insensitive, as MIME's Content-Transfer-Encoding values are (RFC 2045, section 6.1).
    pub(crate) fn from_name(name: &str) -> Option<ContentEncoding> {
        name.eq_ignore_ascii_case("base64")
            .then_some(ContentEncoding::Base64)
    }

    /// The bytes `s` encodes, or `None` when `s` is not in this encoding.
    pub(crate) fn decode(self, s: &str) -> Option<Vec<u8>> {
        match self {
            ContentEncoding::Base64 => decode_base64(s),
        }
    }
}

/// A media type `contentMediaType` names that this library checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentMediaType {
    /// `application/json`, or a type with the `+json` structured syntax suffix (RFC 6839,
    /// section 3.1): the content must be a JSON text.
    Json,
}

impl ContentMediaType {
    /// The media type `name` names, or `None` for one this library does not check. Parameters
    /// (`; charset=utf-8`) are ignored, and type and subtype are case-insensitive (RFC 2045,
    /// section 5.1).
    pub(crate) fn from_name(name: &str) -> Option<ContentMediaType> {
        let essence: &str = name.split(';').next().unwrap_or_default().trim();
        let (kind, subtype) = essence.split_once('/')?;
        let kind: String = kind.trim().to_ascii_lowercase();
        let subtype: String = subtype.trim().to_ascii_lowercase();
        let json: bool = (kind == "application" && subtype == "json")
            || (!kind.is_empty() && subtype.len() > "+json".len() && subtype.ends_with("+json"));
        json.then_some(ContentMediaType::Json)
    }

    /// Whether `content` is a document of this media type.
    pub(crate) fn accepts(self, content: &[u8]) -> bool {
        match self {
            ContentMediaType::Json => {
                serde_json::from_slice::<serde::de::IgnoredAny>(content).is_ok()
            }
        }
    }
}

/// What a schema object's content keywords check: the encoding to decode with (`None`: the
/// identity encoding, as when `contentEncoding` is absent) and the media type the content must
/// have. At least one of them is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Content {
    /// The decoded encoding, with its name as the schema wrote it.
    pub(crate) encoding: Option<(ContentEncoding, Box<str>)>,
    /// The checked media type, with its name as the schema wrote it.
    pub(crate) media_type: Option<(ContentMediaType, Box<str>)>,
}

/// Why a string failed its content keywords, with the keyword's value as the schema wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentFailure<'c> {
    /// The string is not in the encoding `contentEncoding` names.
    Encoding(&'c str),
    /// The (decoded) content is not of the media type `contentMediaType` names.
    MediaType(&'c str),
}

impl Content {
    /// Lower a schema object's `contentEncoding` and `contentMediaType` to a check, or `None`
    /// when they decide nothing: neither is one this library checks, or the media type sits
    /// under an encoding it cannot decode (the content cannot be read, so it stays an
    /// annotation).
    pub(crate) fn lower(encoding: Option<&str>, media_type: Option<&str>) -> Option<Content> {
        let decoded: Option<(ContentEncoding, Box<str>)> =
            encoding.and_then(|name| ContentEncoding::from_name(name).map(|e| (e, name.into())));
        let readable: bool = encoding.is_none() || decoded.is_some();
        let media: Option<(ContentMediaType, Box<str>)> = media_type
            .filter(|_| readable)
            .and_then(|name| ContentMediaType::from_name(name).map(|m| (m, name.into())));
        (decoded.is_some() || media.is_some()).then_some(Content {
            encoding: decoded,
            media_type: media,
        })
    }

    /// The first failure of `s`, or `None` when it passes. A string that does not decode is not
    /// checked against the media type.
    pub(crate) fn check(&self, s: &str) -> Option<ContentFailure<'_>> {
        let content: Cow<'_, [u8]> = match &self.encoding {
            Some((encoding, name)) => match encoding.decode(s) {
                Some(bytes) => Cow::Owned(bytes),
                None => return Some(ContentFailure::Encoding(name)),
            },
            None => Cow::Borrowed(s.as_bytes()),
        };
        match &self.media_type {
            Some((media_type, name)) if !media_type.accepts(&content) => {
                Some(ContentFailure::MediaType(name))
            }
            _ => None,
        }
    }
}

/// The value of one base64 character (RFC 4648, table 1), or `None` outside the alphabet.
const fn base64_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decode `s` as base64 (RFC 4648, section 4): characters of the standard alphabet only, a
/// length that is a multiple of four, and `=` padding only at the end (at most two). Non-zero
/// pad bits are accepted (section 3.5 lets a decoder reject them, but does not require it).
fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let bytes: &[u8] = s.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let padding: usize = bytes.iter().rev().take_while(|&&c| c == b'=').count();
    if padding > 2 {
        return None;
    }
    let data: &[u8] = &bytes[..bytes.len() - padding];
    let mut out: Vec<u8> = Vec::with_capacity(data.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &c in data {
        buffer = (buffer << 6) | u32::from(base64_value(c)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the shift leaves the next byte in the low eight bits"
            )]
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{Content, ContentEncoding, ContentFailure, ContentMediaType, decode_base64};

    #[test]
    fn base64_decodes_unpadded_full_groups() {
        let expected: Option<Vec<u8>> = Some(b"foobar".to_vec());
        let actual: Option<Vec<u8>> = decode_base64("Zm9vYmFy");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_decodes_one_padding_character() {
        let expected: Option<Vec<u8>> = Some(b"fooba".to_vec());
        let actual: Option<Vec<u8>> = decode_base64("Zm9vYmE=");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_decodes_two_padding_characters() {
        let expected: Option<Vec<u8>> = Some(b"foob".to_vec());
        let actual: Option<Vec<u8>> = decode_base64("Zm9vYg==");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_decodes_the_empty_string() {
        let expected: Option<Vec<u8>> = Some(Vec::new());
        let actual: Option<Vec<u8>> = decode_base64("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_accepts_non_zero_pad_bits() {
        let expected: Option<Vec<u8>> = Some(b"foob".to_vec());
        let actual: Option<Vec<u8>> = decode_base64("Zm9vYh==");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_rejects_a_character_outside_the_alphabet() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = decode_base64("eyJmb28i%iJiYXIifQ==");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_rejects_a_length_that_is_not_a_multiple_of_four() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = decode_base64("Zm9vYmE");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_rejects_three_padding_characters() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = decode_base64("Zm9v====");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_rejects_padding_before_the_end() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = decode_base64("Zm==YmFy");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_rejects_whitespace() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = decode_base64("Zm9v\nYmFy");
        assert_eq!(expected, actual);
    }

    #[test]
    fn base64_rejects_the_url_safe_alphabet() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = decode_base64("-_-_");
        assert_eq!(expected, actual);
    }

    #[test]
    fn encoding_names_are_case_insensitive() {
        let expected: Option<ContentEncoding> = Some(ContentEncoding::Base64);
        let actual: Option<ContentEncoding> = ContentEncoding::from_name("BASE64");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_encodings_are_not_decoded() {
        let expected: Option<ContentEncoding> = None;
        let actual: Option<ContentEncoding> = ContentEncoding::from_name("quoted-printable");
        assert_eq!(expected, actual);
    }

    #[test]
    fn media_type_application_json_is_checked() {
        let expected: Option<ContentMediaType> = Some(ContentMediaType::Json);
        let actual: Option<ContentMediaType> = ContentMediaType::from_name("application/json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn media_type_ignores_parameters_and_case() {
        let expected: Option<ContentMediaType> = Some(ContentMediaType::Json);
        let actual: Option<ContentMediaType> =
            ContentMediaType::from_name("Application/JSON; charset=utf-8");
        assert_eq!(expected, actual);
    }

    #[test]
    fn media_type_with_the_json_suffix_is_checked() {
        let expected: Option<ContentMediaType> = Some(ContentMediaType::Json);
        let actual: Option<ContentMediaType> = ContentMediaType::from_name("application/geo+json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn media_type_suffix_alone_is_not_json() {
        let expected: Option<ContentMediaType> = None;
        let actual: Option<ContentMediaType> = ContentMediaType::from_name("application/+json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn other_media_types_are_not_checked() {
        let expected: Option<ContentMediaType> = None;
        let actual: Option<ContentMediaType> = ContentMediaType::from_name("image/png");
        assert_eq!(expected, actual);
    }

    #[test]
    fn lower_is_none_without_a_checked_keyword() {
        let expected: Option<Content> = None;
        let actual: Option<Content> = Content::lower(Some("7bit"), Some("text/html"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn lower_drops_a_media_type_under_an_unknown_encoding() {
        let expected: Option<Content> = None;
        let actual: Option<Content> = Content::lower(Some("base32"), Some("application/json"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn lower_keeps_both_keywords() {
        let expected: Option<Content> = Some(Content {
            encoding: Some((ContentEncoding::Base64, "base64".into())),
            media_type: Some((ContentMediaType::Json, "application/json".into())),
        });
        let actual: Option<Content> = Content::lower(Some("base64"), Some("application/json"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_a_json_string() {
        let content: Content = Content::lower(None, Some("application/json")).expect("checked");
        let expected: Option<ContentFailure<'_>> = None;
        let actual: Option<ContentFailure<'_>> = content.check("{\"foo\": \"bar\"}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_fails_a_string_that_is_not_json() {
        let content: Content = Content::lower(None, Some("application/json")).expect("checked");
        let expected: Option<ContentFailure<'_>> =
            Some(ContentFailure::MediaType("application/json"));
        let actual: Option<ContentFailure<'_>> = content.check("{:}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_fails_a_string_that_does_not_decode() {
        let content: Content =
            Content::lower(Some("base64"), Some("application/json")).expect("checked");
        let expected: Option<ContentFailure<'_>> = Some(ContentFailure::Encoding("base64"));
        let actual: Option<ContentFailure<'_>> = content.check("eyJmb28iOi%iYmFyIn0K");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_fails_decoded_content_that_is_not_json() {
        let content: Content =
            Content::lower(Some("base64"), Some("application/json")).expect("checked");
        let expected: Option<ContentFailure<'_>> =
            Some(ContentFailure::MediaType("application/json"));
        let actual: Option<ContentFailure<'_>> = content.check("ezp9Cg==");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_decoded_json() {
        let content: Content =
            Content::lower(Some("base64"), Some("application/json")).expect("checked");
        let expected: Option<ContentFailure<'_>> = None;
        let actual: Option<ContentFailure<'_>> = content.check("eyJmb28iOiAiYmFyIn0K");
        assert_eq!(expected, actual);
    }
}
