//! The content keywords' checks: `contentEncoding` and `contentMediaType` where the dialect
//! asserts them ([`ContentMode::Assertion`](crate::ContentMode::Assertion)).
//!
//! Built in: `base64` (RFC 4648, section 4) and JSON media types (`application/json` and any
//! `+json` type, RFC 6839). Users add encodings and media types, or remove built-in ones, on
//! [`CompiledSchemaBuilder`](crate::CompiledSchemaBuilder) ([`ContentCodecs`]). Any other
//! encoding or media type stays an annotation, and so does a media type under an encoding that
//! cannot be decoded.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

/// A user's decoder for a `contentEncoding`: the bytes a string encodes, or `None` when the
/// string is not in the encoding.
pub(crate) type Decoder = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// A user's check for a `contentMediaType`: whether the (decoded) content is a document of the
/// media type.
pub(crate) type MediaTypeCheck = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;

/// An encoding `contentEncoding` names that is decoded.
#[derive(Clone)]
pub(crate) enum ContentEncoding {
    /// RFC 4648, section 4: the standard alphabet, padded to a multiple of four characters.
    Base64,
    /// An encoding the user registered.
    User(Decoder),
}

impl PartialEq for ContentEncoding {
    /// A user encoding equals only itself: the same registration (one builder's compilations
    /// share it).
    fn eq(&self, other: &ContentEncoding) -> bool {
        match (self, other) {
            (ContentEncoding::Base64, ContentEncoding::Base64) => true,
            (ContentEncoding::User(a), ContentEncoding::User(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for ContentEncoding {}

impl fmt::Debug for ContentEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentEncoding::Base64 => f.write_str("Base64"),
            ContentEncoding::User(_) => f.write_str("User"),
        }
    }
}

impl ContentEncoding {
    /// The built-in encoding `name` names, or `None` for one this library does not decode.
    /// Names are case-insensitive, as MIME's Content-Transfer-Encoding values are (RFC 2045,
    /// section 6.1).
    pub(crate) fn from_name(name: &str) -> Option<ContentEncoding> {
        name.eq_ignore_ascii_case("base64")
            .then_some(ContentEncoding::Base64)
    }

    /// The bytes `s` encodes, or `None` when `s` is not in this encoding.
    pub(crate) fn decode(&self, s: &str) -> Option<Vec<u8>> {
        match self {
            ContentEncoding::Base64 => decode_base64(s),
            ContentEncoding::User(decode) => decode(s),
        }
    }
}

/// A media type `contentMediaType` names that is checked.
#[derive(Clone)]
pub(crate) enum ContentMediaType {
    /// `application/json`, or a type with the `+json` structured syntax suffix (RFC 6839,
    /// section 3.1): the content must be a JSON text.
    Json,
    /// A media type the user registered.
    User(MediaTypeCheck),
}

impl PartialEq for ContentMediaType {
    /// A user media type equals only itself (see [`ContentEncoding`]'s `eq`).
    fn eq(&self, other: &ContentMediaType) -> bool {
        match (self, other) {
            (ContentMediaType::Json, ContentMediaType::Json) => true,
            (ContentMediaType::User(a), ContentMediaType::User(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for ContentMediaType {}

impl fmt::Debug for ContentMediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentMediaType::Json => f.write_str("Json"),
            ContentMediaType::User(_) => f.write_str("User"),
        }
    }
}

/// A media type's essence (RFC 2045, section 5.1): `type/subtype` in lowercase, without
/// parameters (`; charset=utf-8`) or the whitespace around them.
fn essence(name: &str) -> String {
    let essence: &str = name.split(';').next().unwrap_or_default().trim();
    match essence.split_once('/') {
        Some((kind, subtype)) => format!(
            "{}/{}",
            kind.trim().to_ascii_lowercase(),
            subtype.trim().to_ascii_lowercase()
        ),
        None => essence.to_ascii_lowercase(),
    }
}

impl ContentMediaType {
    /// The built-in media type `name` names, or `None` for one this library does not check.
    /// Parameters are ignored and type and subtype are case-insensitive (see [`essence`]).
    pub(crate) fn from_name(name: &str) -> Option<ContentMediaType> {
        let essence: String = essence(name);
        let (kind, subtype) = essence.split_once('/')?;
        let json: bool = (kind == "application" && subtype == "json")
            || (!kind.is_empty() && subtype.len() > "+json".len() && subtype.ends_with("+json"));
        json.then_some(ContentMediaType::Json)
    }

    /// Whether `content` is a document of this media type.
    pub(crate) fn accepts(&self, content: &[u8]) -> bool {
        match self {
            ContentMediaType::Json => {
                serde_json::from_slice::<serde::de::IgnoredAny>(content).is_ok()
            }
            ContentMediaType::User(check) => check(content),
        }
    }
}

/// The encodings and media types the user registered or removed, on top of the built-in ones:
/// each by its normalized name (an encoding in lowercase, a media type by its [`essence`]),
/// with its check, or `None` for a removed name, which stays an annotation.
#[derive(Clone, Default)]
pub(crate) struct ContentCodecs {
    encodings: Vec<(Box<str>, Option<Decoder>)>,
    media_types: Vec<(Box<str>, Option<MediaTypeCheck>)>,
}

impl fmt::Debug for ContentCodecs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContentCodecs")
            .field("encodings", &registered(&self.encodings))
            .field("media_types", &registered(&self.media_types))
            .finish()
    }
}

/// Each entry's name and whether it is registered (`true`) or removed (`false`).
fn registered<C>(entries: &[(Box<str>, Option<C>)]) -> Vec<(&str, bool)> {
    entries
        .iter()
        .map(|(name, check)| (&**name, check.is_some()))
        .collect()
}

impl ContentCodecs {
    /// Registers (`Some`) or removes (`None`) the encoding `name`, replacing an earlier entry
    /// for the same name.
    pub(crate) fn set_encoding(&mut self, name: &str, decode: Option<Decoder>) {
        let key: Box<str> = name.to_ascii_lowercase().into_boxed_str();
        self.encodings.retain(|(name, _)| *name != key);
        self.encodings.push((key, decode));
    }

    /// Registers (`Some`) or removes (`None`) the media type `name`, replacing an earlier entry
    /// for the same essence.
    pub(crate) fn set_media_type(&mut self, name: &str, check: Option<MediaTypeCheck>) {
        let key: Box<str> = essence(name).into_boxed_str();
        self.media_types.retain(|(name, _)| *name != key);
        self.media_types.push((key, check));
    }

    /// The encoding `name` names: the user's entry for it, else the built-in one.
    fn encoding(&self, name: &str) -> Option<ContentEncoding> {
        match self
            .encodings
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
        {
            Some((_, decode)) => decode.clone().map(ContentEncoding::User),
            None => ContentEncoding::from_name(name),
        }
    }

    /// The media type `name` names: the user's entry for its essence, else the built-in one.
    fn media_type(&self, name: &str) -> Option<ContentMediaType> {
        if self.media_types.is_empty() {
            return ContentMediaType::from_name(name);
        }
        let key: String = essence(name);
        match self.media_types.iter().find(|(entry, _)| **entry == *key) {
            Some((_, check)) => check.clone().map(ContentMediaType::User),
            None => ContentMediaType::from_name(name),
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
    /// Lower a schema object's `contentEncoding` and `contentMediaType` to a check, with the
    /// encodings and media types `codecs` registers or removes, or `None` when they decide
    /// nothing: neither is one that is checked, or the media type sits under an encoding that
    /// cannot be decoded (the content cannot be read, so it stays an annotation).
    pub(crate) fn lower(
        encoding: Option<&str>,
        media_type: Option<&str>,
        codecs: &ContentCodecs,
    ) -> Option<Content> {
        let decoded: Option<(ContentEncoding, Box<str>)> =
            encoding.and_then(|name| codecs.encoding(name).map(|e| (e, name.into())));
        let readable: bool = encoding.is_none() || decoded.is_some();
        let media: Option<(ContentMediaType, Box<str>)> = media_type
            .filter(|_| readable)
            .and_then(|name| codecs.media_type(name).map(|m| (m, name.into())));
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
    use super::{
        Content, ContentCodecs, ContentEncoding, ContentFailure, ContentMediaType, decode_base64,
    };

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
        let actual: Option<Content> =
            Content::lower(Some("7bit"), Some("text/html"), &ContentCodecs::default());
        assert_eq!(expected, actual);
    }

    #[test]
    fn lower_drops_a_media_type_under_an_unknown_encoding() {
        let expected: Option<Content> = None;
        let actual: Option<Content> = Content::lower(
            Some("base32"),
            Some("application/json"),
            &ContentCodecs::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn lower_keeps_both_keywords() {
        let expected: Option<Content> = Some(Content {
            encoding: Some((ContentEncoding::Base64, "base64".into())),
            media_type: Some((ContentMediaType::Json, "application/json".into())),
        });
        let actual: Option<Content> = Content::lower(
            Some("base64"),
            Some("application/json"),
            &ContentCodecs::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_a_json_string() {
        let content: Content =
            Content::lower(None, Some("application/json"), &ContentCodecs::default())
                .expect("checked");
        let expected: Option<ContentFailure<'_>> = None;
        let actual: Option<ContentFailure<'_>> = content.check("{\"foo\": \"bar\"}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_fails_a_string_that_is_not_json() {
        let content: Content =
            Content::lower(None, Some("application/json"), &ContentCodecs::default())
                .expect("checked");
        let expected: Option<ContentFailure<'_>> =
            Some(ContentFailure::MediaType("application/json"));
        let actual: Option<ContentFailure<'_>> = content.check("{:}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_fails_a_string_that_does_not_decode() {
        let content: Content = Content::lower(
            Some("base64"),
            Some("application/json"),
            &ContentCodecs::default(),
        )
        .expect("checked");
        let expected: Option<ContentFailure<'_>> = Some(ContentFailure::Encoding("base64"));
        let actual: Option<ContentFailure<'_>> = content.check("eyJmb28iOi%iYmFyIn0K");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_fails_decoded_content_that_is_not_json() {
        let content: Content = Content::lower(
            Some("base64"),
            Some("application/json"),
            &ContentCodecs::default(),
        )
        .expect("checked");
        let expected: Option<ContentFailure<'_>> =
            Some(ContentFailure::MediaType("application/json"));
        let actual: Option<ContentFailure<'_>> = content.check("ezp9Cg==");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_decoded_json() {
        let content: Content = Content::lower(
            Some("base64"),
            Some("application/json"),
            &ContentCodecs::default(),
        )
        .expect("checked");
        let expected: Option<ContentFailure<'_>> = None;
        let actual: Option<ContentFailure<'_>> = content.check("eyJmb28iOiAiYmFyIn0K");
        assert_eq!(expected, actual);
    }
}
