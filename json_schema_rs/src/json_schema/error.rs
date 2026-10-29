//! Errors when parsing JSON Schema.

use crate::error_list::ErrorList;
use crate::json_pointer::JsonPointer;
use std::fmt;

/// Result type for JSON Schema parsing operations.
pub type JsonSchemaParseResult<T> = Result<T, JsonSchemaParseError>;

/// Every problem parsing one schema document found, one [`JsonSchemaParseError`] each.
pub type JsonSchemaParseErrors = ErrorList<JsonSchemaParseError>;

/// Error when parsing a JSON Schema with the given settings.
#[derive(Debug)]
pub enum JsonSchemaParseError {
    /// JSON or serde error (invalid JSON, wrong types, etc.).
    Serde(serde_json::Error),
    /// Strict ingestion was enabled and a schema object has a key that the document's draft
    /// does not define as a keyword.
    UnknownField {
        /// The unknown key.
        key: String,
        /// JSON Pointer to the schema object that has the key (the root is the empty pointer).
        path: JsonPointer,
    },
    /// A keyword's value is not one the document's dialect allows, e.g. `"minLength": 2.0` under
    /// [`IntegerSemantics::Lexical`](super::IntegerSemantics::Lexical) (draft 4).
    InvalidKeywordValue {
        /// The keyword.
        keyword: String,
        /// JSON Pointer to the schema object that has the keyword (the root is the empty pointer).
        path: JsonPointer,
        /// The value as written.
        value: serde_json::Value,
        /// What the dialect allows.
        expected: &'static str,
    },
    /// I/O error when reading from a reader or file.
    Io(std::io::Error),
    /// The document is a boolean schema (`true` / `false`) but a schema object was asked for
    /// ([`JsonSchema`](super::JsonSchema) parsing). [`Schema`](super::Schema) parsing accepts it.
    BooleanRoot {
        /// The boolean schema's value.
        value: bool,
    },
    /// `$schema` is not a recognised metaschema URI and
    /// [`UnknownSchemaUri::Reject`](super::settings::UnknownSchemaUri::Reject) is set.
    UnknownSchemaUri {
        /// The unrecognised `$schema` value.
        uri: String,
    },
}

impl fmt::Display for JsonSchemaParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonSchemaParseError::Serde(e) => write!(f, "invalid JSON Schema: {e}"),
            JsonSchemaParseError::UnknownField { key, path } => {
                write!(
                    f,
                    "unknown keyword \"{key}\" at {}",
                    path.display_root_or_path()
                )
            }
            JsonSchemaParseError::InvalidKeywordValue {
                keyword,
                path,
                value,
                expected,
            } => write!(
                f,
                "invalid \"{keyword}\" value {value} at {}: expected {expected}",
                path.display_root_or_path()
            ),
            JsonSchemaParseError::Io(e) => write!(f, "io error: {e}"),
            JsonSchemaParseError::BooleanRoot { value } => write!(
                f,
                "the document is the boolean schema `{value}`, not a schema object"
            ),
            JsonSchemaParseError::UnknownSchemaUri { uri } => {
                write!(f, "unknown $schema \"{uri}\"")
            }
        }
    }
}

impl std::error::Error for JsonSchemaParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JsonSchemaParseError::Serde(e) => Some(e),
            JsonSchemaParseError::UnknownField { .. }
            | JsonSchemaParseError::InvalidKeywordValue { .. }
            | JsonSchemaParseError::UnknownSchemaUri { .. }
            | JsonSchemaParseError::BooleanRoot { .. } => None,
            JsonSchemaParseError::Io(e) => Some(e),
        }
    }
}

impl From<serde_json::Error> for JsonSchemaParseError {
    fn from(e: serde_json::Error) -> Self {
        JsonSchemaParseError::Serde(e)
    }
}

impl From<std::io::Error> for JsonSchemaParseError {
    fn from(e: std::io::Error) -> Self {
        JsonSchemaParseError::Io(e)
    }
}
