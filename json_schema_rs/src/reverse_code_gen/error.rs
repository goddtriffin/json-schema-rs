//! Errors from building a JSON Schema from Rust types.

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
    /// A type's `#[json_schema(id = "...")]` is not a URI reference (RFC 3986 / RFC 3987).
    InvalidResourceId {
        /// [`super::ToJsonSchema::schema_id`] of the type.
        schema_id: String,
        /// The `$id` as written.
        id: String,
        /// Why it is not a URI reference.
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
            ReverseCodeGenError::InvalidResourceId {
                schema_id,
                id,
                reason,
            } => write!(
                f,
                "`{schema_id}` has $id {id:?}, which is not a URI reference: {reason}"
            ),
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
}
