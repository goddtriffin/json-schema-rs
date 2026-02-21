//! In-memory representation of JSON Schema for codegen.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Schema helper with `deny_unknown_fields`: same shape as our schema model but with `#[serde(deny_unknown_fields)]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DenyUnknownFieldsJsonSchema {
    #[serde(
        default,
        deserialize_with = "super::parser::deserialize_type_optional_deny_unknown_fields",
        rename = "type"
    )]
    pub(crate) type_: Option<String>,
    #[serde(default)]
    pub(crate) properties: Option<BTreeMap<String, DenyUnknownFieldsJsonSchema>>,
    #[serde(default)]
    pub(crate) required: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) title: Option<String>,
}

/// Converts a strict (deny-unknown-fields) deserialized helper into the public [`JsonSchema`] model.
pub(crate) fn deny_unknown_fields_helper_to_schema(h: DenyUnknownFieldsJsonSchema) -> JsonSchema {
    let properties: BTreeMap<String, JsonSchema> = h
        .properties
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, deny_unknown_fields_helper_to_schema(v)))
        .collect();
    JsonSchema {
        type_: h.type_,
        properties,
        required: h.required,
        title: h.title,
    }
}

/// Schema model used for code generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonSchema {
    /// Schema type; `object` and `string` drive codegen; others are ignored.
    pub type_: Option<String>,

    /// Object properties (only when type is "object"). Default empty; use `BTreeMap` for stable ordering.
    pub properties: BTreeMap<String, JsonSchema>,

    /// Required property names at this object level. When absent, all properties are optional.
    pub required: Option<Vec<String>>,

    /// Used for struct naming when present (`PascalCase`).
    pub title: Option<String>,
}

impl JsonSchema {
    /// Returns true if this schema is an object with properties (for codegen).
    #[must_use]
    pub(crate) fn is_object_with_properties(&self) -> bool {
        self.type_.as_deref() == Some("object") && !self.properties.is_empty()
    }

    /// Returns true if this schema is type "string".
    #[must_use]
    pub(crate) fn is_string(&self) -> bool {
        self.type_.as_deref() == Some("string")
    }

    /// Returns true if the given property name is required at this object level.
    #[must_use]
    pub(crate) fn is_required(&self, name: &str) -> bool {
        self.required
            .as_ref()
            .is_some_and(|r| r.iter().any(|s| s == name))
    }
}

#[cfg(test)]
mod tests {
    use super::JsonSchema;

    #[test]
    fn is_object_with_properties() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_object_with_properties(),
            {
                s.type_ = Some("object".to_string());
                s.is_object_with_properties()
            },
            {
                s.properties.insert("x".to_string(), JsonSchema::default());
                s.is_object_with_properties()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_string() {
        let mut s = JsonSchema::default();
        let actual = [s.is_string(), {
            s.type_ = Some("string".to_string());
            s.is_string()
        }];
        let expected = [false, true];
        assert_eq!(expected, actual);
    }
}
