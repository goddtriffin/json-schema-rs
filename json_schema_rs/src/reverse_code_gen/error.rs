//! Errors from building a JSON Schema from Rust types.

use super::settings::TargetDialect;
use std::fmt;

/// Result of [`super::ToJsonSchema::json_schema`]: the schema, or every problem found while
/// building it.
pub type ReverseCodeGenResult = Result<crate::json_schema::JsonSchema, Vec<ReverseCodeGenError>>;

/// A problem that keeps a set of Rust types from producing a valid JSON Schema document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReverseCodeGenError {
    /// Two different types with `#[json_schema(id = "...")]` resolve to the same URI with
    /// different schemas. A document may not hold two resources with one URI (2020-12 core
    /// §8.2.1); identical schemas are merged instead.
    DuplicateResourceId {
        /// [`super::ToJsonSchema::schema_id`] of the type embedded first.
        first: String,
        /// Its `$id` as written.
        first_id: String,
        /// [`super::ToJsonSchema::schema_id`] of the type that collides with it.
        second: String,
        /// Its `$id` as written.
        second_id: String,
    },
    /// Two schemas in one schema resource declare the same plain-name anchor: with a
    /// fragment-only `$id` (`#a`) in draft 4-7, with `$anchor` (which such an `$id` is written
    /// as) from 2019-09. A resource may not hold one anchor at two locations (the registry's
    /// `DuplicateAnchor`); equal entries of one name are merged instead.
    DuplicateAnchor {
        /// JSON Pointer (as a `#` fragment) of the schema that declares it first: in draft 4-7
        /// always a hand-written entry.
        first: String,
        /// Where it is declared again: in draft 4-7 the
        /// [`super::ToJsonSchema::schema_id`] of the type (or the hoisted definition), from
        /// 2019-09 a JSON Pointer like `first`.
        second: String,
        /// The anchor name, without `#`.
        anchor: String,
    },
    /// A type's `#[json_schema(id = "...")]` is not a URI reference (RFC 3986 / RFC 3987).
    InvalidResourceId {
        /// [`super::ToJsonSchema::schema_id`] of the type.
        schema_id: String,
        /// The `$id` as written.
        id: String,
        /// Why it is not a URI reference.
        reason: String,
    },
    /// A schema written out as JSON ([`super::schema_from_json`]) is not a JSON Schema object.
    InvalidSchema {
        /// The JSON as written.
        json: String,
        /// Why it is not a schema object.
        reason: String,
    },
    /// The target dialect cannot express a keyword of the schema exactly, and no equivalent
    /// schema exists, so the generator refuses instead of approximating.
    UnrepresentableKeyword {
        /// The dialect being written.
        dialect: TargetDialect,
        /// The keyword (or, for a `null` type no keyword spells, `type`).
        keyword: String,
        /// JSON Pointer of the schema object holding it, in the generated document.
        pointer: String,
    },
    /// [`ReverseCodeGenSettings::definitions_path`](super::ReverseCodeGenSettings::definitions_path)
    /// is not a usable JSON Pointer for the target dialect.
    InvalidDefinitionsPath {
        /// The path as written.
        path: String,
        /// Why it cannot hold definitions.
        reason: String,
    },
    /// A non-object already sits where the definitions go (or on the way there), or a name the
    /// generator could not avoid is taken, so writing them there would overwrite it.
    DefinitionsPathOccupied {
        /// The normalized path.
        path: String,
    },
    /// A type with `$id` is a schema resource that must sit in its dialect's own definitions
    /// container to be found, and a custom
    /// [`definitions_path`](super::ReverseCodeGenSettings::definitions_path) puts it elsewhere.
    ResourceOutsideDefinitions {
        /// The normalized path.
        path: String,
        /// [`super::ToJsonSchema::schema_id`] of a type with `$id`.
        schema_id: String,
    },
    /// [`MetaSchema::Uri`](super::MetaSchema::Uri) is not an absolute URI.
    InvalidMetaSchemaUri {
        /// The URI as written.
        uri: String,
        /// Why it is not an absolute URI.
        reason: String,
    },
}

impl fmt::Display for ReverseCodeGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReverseCodeGenError::DuplicateResourceId {
                first,
                first_id,
                second,
                second_id,
            } => write!(
                f,
                "`{second}` ($id {second_id:?}) resolves to the same URI as `{first}` ($id \
                 {first_id:?}) but has a different schema"
            ),
            ReverseCodeGenError::DuplicateAnchor {
                first,
                second,
                anchor,
            } => write!(
                f,
                "`{second}` declares the anchor {anchor:?}, which `{first}` already declares in \
                 the same schema resource"
            ),
            ReverseCodeGenError::InvalidResourceId {
                schema_id,
                id,
                reason,
            } => write!(
                f,
                "`{schema_id}` has $id {id:?}, which is not a URI reference: {reason}"
            ),
            ReverseCodeGenError::InvalidSchema { json, reason } => {
                write!(f, "{json} is not a JSON Schema object: {reason}")
            }
            ReverseCodeGenError::UnrepresentableKeyword {
                dialect,
                keyword,
                pointer,
            } => write!(
                f,
                "`{keyword}` at {pointer:?} cannot be written exactly in {dialect}"
            ),
            ReverseCodeGenError::InvalidDefinitionsPath { path, reason } => {
                write!(f, "definitions path {path:?} is not usable: {reason}")
            }
            ReverseCodeGenError::DefinitionsPathOccupied { path } => {
                write!(f, "definitions path {path:?} already holds a value")
            }
            ReverseCodeGenError::ResourceOutsideDefinitions { path, schema_id } => write!(
                f,
                "`{schema_id}` has an $id, so it cannot be placed under definitions path {path:?}"
            ),
            ReverseCodeGenError::InvalidMetaSchemaUri { uri, reason } => {
                write!(f, "meta schema {uri:?} is not an absolute URI: {reason}")
            }
        }
    }
}

impl std::error::Error for ReverseCodeGenError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_resource_id_display_names_both_types_and_ids() {
        let expected: String = "`b::Node` ($id \"./node.json\") resolves to the same URI as \
                                `a::Node` ($id \"node.json\") but has a different schema"
            .to_string();
        let actual: String = ReverseCodeGenError::DuplicateResourceId {
            first: "a::Node".to_string(),
            first_id: "node.json".to_string(),
            second: "b::Node".to_string(),
            second_id: "./node.json".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn duplicate_anchor_display_names_both_declarations_and_the_anchor() {
        let expected: String = "`a::A` declares the anchor \"a\", which `#/definitions/H` already \
                                declares in the same schema resource"
            .to_string();
        let actual: String = ReverseCodeGenError::DuplicateAnchor {
            first: "#/definitions/H".to_string(),
            second: "a::A".to_string(),
            anchor: "a".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn invalid_resource_id_display_names_the_type_and_reason() {
        let expected: String =
            "`a::Node` has $id \"a b\", which is not a URI reference: bad".to_string();
        let actual: String = ReverseCodeGenError::InvalidResourceId {
            schema_id: "a::Node".to_string(),
            id: "a b".to_string(),
            reason: "bad".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn invalid_schema_display_names_the_json_and_reason() {
        let expected: String = "[1] is not a JSON Schema object: bad".to_string();
        let actual: String = ReverseCodeGenError::InvalidSchema {
            json: "[1]".to_string(),
            reason: "bad".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn unrepresentable_keyword_display_names_keyword_pointer_and_dialect() {
        let expected: String =
            "`unevaluatedProperties` at \"/allOf/0\" cannot be written exactly in draft-07"
                .to_string();
        let actual: String = ReverseCodeGenError::UnrepresentableKeyword {
            dialect: TargetDialect::Draft07,
            keyword: "unevaluatedProperties".to_string(),
            pointer: "/allOf/0".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn invalid_definitions_path_display_names_the_path_and_reason() {
        let expected: String = "definitions path \"\" is not usable: it is empty".to_string();
        let actual: String = ReverseCodeGenError::InvalidDefinitionsPath {
            path: String::new(),
            reason: "it is empty".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn definitions_path_occupied_display_names_the_path() {
        let expected: String = "definitions path \"/title\" already holds a value".to_string();
        let actual: String = ReverseCodeGenError::DefinitionsPathOccupied {
            path: "/title".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_outside_definitions_display_names_the_type_and_path() {
        let expected: String =
            "`a::Node` has an $id, so it cannot be placed under definitions path \"/x\""
                .to_string();
        let actual: String = ReverseCodeGenError::ResourceOutsideDefinitions {
            path: "/x".to_string(),
            schema_id: "a::Node".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn invalid_meta_schema_uri_display_names_the_uri_and_reason() {
        let expected: String = "meta schema \"x\" is not an absolute URI: no scheme".to_string();
        let actual: String = ReverseCodeGenError::InvalidMetaSchemaUri {
            uri: "x".to_string(),
            reason: "no scheme".to_string(),
        }
        .to_string();
        assert_eq!(expected, actual);
    }
}
