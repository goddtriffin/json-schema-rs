//! In-memory representation of JSON Schema for codegen.

use serde::Deserialize;
use std::collections::BTreeMap;

/// JSON Schema type keyword: either a single type string or an array of types (draft 2020-12).
/// First type in the array is used; codegen uses `object` and `string`.
fn deserialize_type_optional<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TypeOrArray {
        Single(String),
        Array(Vec<String>),
    }
    let value: TypeOrArray = Deserialize::deserialize(deserializer)?;
    let first = match value {
        TypeOrArray::Single(s) => Some(s),
        TypeOrArray::Array(a) => a.into_iter().next(),
    };
    Ok(first)
}

/// Schema model used for code generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    /// Schema type; `object` and `string` drive codegen; others are ignored.
    pub type_: Option<String>,

    /// Object properties (only when type is "object"). Default empty; use `BTreeMap` for stable ordering.
    pub properties: BTreeMap<String, Schema>,

    /// Required property names at this object level. When absent, all properties are optional.
    pub required: Option<Vec<String>>,

    /// Used for struct naming when present (`PascalCase`).
    pub title: Option<String>,
}

impl<'de> Deserialize<'de> for Schema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SchemaHelper {
            #[serde(default, deserialize_with = "deserialize_type_optional")]
            #[serde(rename = "type")]
            type_: Option<String>,
            #[serde(default)]
            properties: Option<BTreeMap<String, Schema>>,
            #[serde(default)]
            required: Option<Vec<String>>,
            #[serde(default)]
            title: Option<String>,
        }
        let h: SchemaHelper = SchemaHelper::deserialize(deserializer)?;
        Ok(Schema {
            type_: h.type_,
            properties: h.properties.unwrap_or_default(),
            required: h.required,
            title: h.title,
        })
    }
}

impl Schema {
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
    use super::Schema;
    use std::collections::BTreeMap;

    #[test]
    fn deserialize_simple_object_schema() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let expected: Schema = Schema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    Schema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            required: None,
            title: None,
        };
        let actual: Schema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_with_required() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"},"y":{"type":"string"}},"required":["x"]}"#;
        let expected: Schema = Schema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "x".to_string(),
                    Schema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m.insert(
                    "y".to_string(),
                    Schema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            required: Some(vec!["x".to_string()]),
            title: None,
        };
        let actual: Schema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_ignores_unknown_keys() {
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let expected: Schema = Schema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
        };
        let actual: Schema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_type_array_takes_first() {
        let json = r#"{"type":["string", "null"],"properties":{}}"#;
        let expected: Schema = Schema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
        };
        let actual: Schema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_object_with_properties() {
        let mut s = Schema::default();
        let actual = [
            s.is_object_with_properties(),
            {
                s.type_ = Some("object".to_string());
                s.is_object_with_properties()
            },
            {
                s.properties.insert("x".to_string(), Schema::default());
                s.is_object_with_properties()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_string() {
        let mut s = Schema::default();
        let actual = [s.is_string(), {
            s.type_ = Some("string".to_string());
            s.is_string()
        }];
        let expected = [false, true];
        assert_eq!(expected, actual);
    }
}
