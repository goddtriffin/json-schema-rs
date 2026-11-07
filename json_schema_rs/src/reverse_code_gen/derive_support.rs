//! Run-time support for `#[derive(ToJsonSchema)]` (`json_schema_rs_macro`).
//!
//! The derive emits static data (one [`FieldSpec`] per struct field, one [`EnumValue`] per
//! value-enum variant, one [`SchemaFn`] per union variant) and a call to one of the
//! non-generic builders here, so the code it generates for a type is the same size whatever
//! the type's width: a struct with 800 fields compiles one call, not 800 inlined blocks.

use super::SchemaGenerator;
use crate::json_schema::{JsonSchema, Schema};
use std::collections::BTreeMap;

/// A type's `ToJsonSchema::json_schema_with`, as a fn pointer.
pub type SchemaFn = fn(&mut SchemaGenerator) -> JsonSchema;

/// A numeric attribute literal, kept exactly as written: integers stay integers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberLiteral {
    /// A non-negative integer literal.
    Unsigned(u64),
    /// A negative integer literal.
    Signed(i64),
    /// A float literal.
    Float(f64),
}

impl NumberLiteral {
    /// The literal as a JSON number; `None` for a float that is not finite.
    #[must_use]
    pub fn to_number(self) -> Option<serde_json::Number> {
        match self {
            NumberLiteral::Unsigned(n) => Some(serde_json::Number::from(n)),
            NumberLiteral::Signed(n) => Some(serde_json::Number::from(n)),
            NumberLiteral::Float(f) => serde_json::Number::from_f64(f),
        }
    }
}

/// The literal of a field's `#[json_schema(default = ...)]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultLiteral {
    /// A string literal.
    String(&'static str),
    /// A boolean literal.
    Bool(bool),
    /// An integer or float literal.
    Number(NumberLiteral),
}

impl DefaultLiteral {
    /// The literal as a JSON value; `None` for a float that is not finite.
    #[must_use]
    pub fn to_value(self) -> Option<serde_json::Value> {
        match self {
            DefaultLiteral::String(s) => Some(serde_json::Value::String(s.to_string())),
            DefaultLiteral::Bool(b) => Some(serde_json::Value::Bool(b)),
            DefaultLiteral::Number(n) => n.to_number().map(serde_json::Value::Number),
        }
    }
}

/// One property of a derived struct: its key, its type's schema and the field's attributes.
///
/// Each attribute that is present replaces the corresponding keyword of the type's schema;
/// an absent one keeps it.
#[derive(Debug, Clone, Copy)]
pub struct FieldSpec {
    /// The JSON property key (`#[serde(rename)]` or the field name).
    pub key: &'static str,
    /// The schema of the field's type (one `Option` stripped for an optional field).
    pub schema: SchemaFn,
    /// Whether the key is in `required`.
    pub required: bool,
    /// The field's `///` doc lines, as written (see [`doc_description`]).
    pub doc: &'static [&'static str],
    /// `#[json_schema(minimum = N)]`.
    pub minimum: Option<NumberLiteral>,
    /// `#[json_schema(maximum = N)]`.
    pub maximum: Option<NumberLiteral>,
    /// `#[json_schema(exclusive_minimum = N)]`.
    pub exclusive_minimum: Option<NumberLiteral>,
    /// `#[json_schema(exclusive_maximum = N)]`.
    pub exclusive_maximum: Option<NumberLiteral>,
    /// `#[json_schema(multiple_of = N)]`.
    pub multiple_of: Option<NumberLiteral>,
    /// `#[json_schema(min_items = N)]`.
    pub min_items: Option<u64>,
    /// `#[json_schema(max_items = N)]`.
    pub max_items: Option<u64>,
    /// `#[json_schema(unique_items)]`: sets `uniqueItems: true`.
    pub unique_items: bool,
    /// `#[json_schema(min_length = N)]`.
    pub min_length: Option<u64>,
    /// `#[json_schema(max_length = N)]`.
    pub max_length: Option<u64>,
    /// `#[json_schema(pattern = "...")]`.
    pub pattern: Option<&'static str>,
    /// `#[json_schema(deprecated = true)]`: sets `deprecated: true`.
    pub deprecated: bool,
    /// `#[json_schema(default = ...)]`: replaces the type's `default` (a non-finite float
    /// literal removes it).
    pub default: Option<DefaultLiteral>,
}

/// The `properties` and `required` of a derived struct, from its fields in declaration order.
///
/// Each field's schema is `(field.schema)(generator)` with the field's attributes applied on
/// top. `required` lists the required keys in field order, and is `None` when there are none.
pub fn object_properties(
    generator: &mut SchemaGenerator,
    fields: &[FieldSpec],
) -> (BTreeMap<String, Schema>, Option<Vec<String>>) {
    let mut properties: BTreeMap<String, Schema> = BTreeMap::new();
    let mut required: Vec<String> = Vec::new();
    for field in fields {
        let mut schema: JsonSchema = (field.schema)(generator);
        schema.description = doc_description(field.doc).or(schema.description);
        schema.minimum = field
            .minimum
            .and_then(NumberLiteral::to_number)
            .or(schema.minimum);
        schema.maximum = field
            .maximum
            .and_then(NumberLiteral::to_number)
            .or(schema.maximum);
        schema.exclusive_minimum = field
            .exclusive_minimum
            .and_then(NumberLiteral::to_number)
            .or(schema.exclusive_minimum);
        schema.exclusive_maximum = field
            .exclusive_maximum
            .and_then(NumberLiteral::to_number)
            .or(schema.exclusive_maximum);
        schema.multiple_of = field
            .multiple_of
            .and_then(NumberLiteral::to_number)
            .or(schema.multiple_of);
        schema.min_items = field.min_items.or(schema.min_items);
        schema.max_items = field.max_items.or(schema.max_items);
        schema.unique_items = field.unique_items.then_some(true).or(schema.unique_items);
        schema.min_length = field.min_length.or(schema.min_length);
        schema.max_length = field.max_length.or(schema.max_length);
        schema.pattern = field.pattern.map(str::to_string).or(schema.pattern);
        schema.deprecated = field.deprecated.then_some(true).or(schema.deprecated);
        if let Some(default) = field.default {
            schema.default_value = default.to_value();
        }
        if field.required {
            required.push(field.key.to_string());
        }
        properties.insert(field.key.to_string(), Schema::from(schema));
    }
    let required: Option<Vec<String>> = (!required.is_empty()).then_some(required);
    (properties, required)
}

/// The description from doc lines: each line trimmed, joined with `\n`, leading and trailing
/// blank lines removed; `None` when no text remains.
#[must_use]
pub fn doc_description(lines: &[&str]) -> Option<String> {
    let trimmed: Vec<&str> = lines.iter().map(|line: &&str| line.trim()).collect();
    let joined: String = trimmed.join("\n").trim().to_string();
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// The `additionalProperties` of a struct whose `#[serde(flatten)]` map has values of the
/// type `value`: that type's schema, or `None` when it is `{}` (a map of any value adds no
/// constraint).
pub fn flattened_map_additional_properties(
    generator: &mut SchemaGenerator,
    value: SchemaFn,
) -> Option<Schema> {
    let value_schema: JsonSchema = value(generator);
    if value_schema == JsonSchema::default() {
        None
    } else {
        Some(Schema::from(value_schema))
    }
}

/// The value of one variant of a derived value enum.
#[derive(Debug, Clone, Copy)]
pub enum EnumValue {
    /// The variant's serde name, as a string.
    Name(&'static str),
    /// The variant's `#[json_schema(value = <JSON literal>)]`.
    Json(fn() -> serde_json::Value),
}

impl EnumValue {
    /// The variant's JSON value.
    #[must_use]
    pub fn to_value(self) -> serde_json::Value {
        match self {
            EnumValue::Name(name) => serde_json::Value::String(name.to_string()),
            EnumValue::Json(value) => value(),
        }
    }
}

/// The `enum` and `const` of a derived value enum: `const` for one variant, else `enum` of
/// every variant's value in order (`[]` for no variants).
#[must_use]
pub fn value_enum_keywords(
    values: &[EnumValue],
) -> (Option<Vec<serde_json::Value>>, Option<serde_json::Value>) {
    if let [value] = values {
        (None, Some(value.to_value()))
    } else {
        (
            Some(values.iter().map(|value| value.to_value()).collect()),
            None,
        )
    }
}

/// The schemas of a derived union's variant payloads, in variant order.
pub fn payload_schemas(generator: &mut SchemaGenerator, payloads: &[SchemaFn]) -> Vec<Schema> {
    payloads
        .iter()
        .map(|payload: &SchemaFn| Schema::from(payload(generator)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        DefaultLiteral, EnumValue, FieldSpec, NumberLiteral, SchemaFn, doc_description,
        flattened_map_additional_properties, object_properties, payload_schemas,
        value_enum_keywords,
    };
    use crate::json_schema::{JsonSchema, JsonType, Schema};
    use crate::reverse_code_gen::{SchemaGenerator, ToJsonSchema};
    use std::collections::BTreeMap;

    type Properties = (BTreeMap<String, Schema>, Option<Vec<String>>);

    /// A schema with every keyword a field attribute can replace set: the base every override
    /// test applies its field attributes to.
    fn base_schema(_generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Integer.into()),
            description: Some("base".to_string()),
            minimum: Some(serde_json::Number::from(3_u64)),
            maximum: Some(serde_json::Number::from(4_u64)),
            exclusive_minimum: Some(serde_json::Number::from(1_u64)),
            exclusive_maximum: Some(serde_json::Number::from(9_u64)),
            multiple_of: Some(serde_json::Number::from(2_u64)),
            min_items: Some(1),
            max_items: Some(2),
            unique_items: Some(false),
            min_length: Some(3),
            max_length: Some(4),
            pattern: Some("base".to_string()),
            deprecated: Some(false),
            default_value: Some(serde_json::Value::Bool(false)),
            ..JsonSchema::default()
        }
    }

    /// A field with no attributes over [`base_schema`].
    const PLAIN: FieldSpec = FieldSpec {
        key: "k",
        schema: base_schema,
        required: false,
        doc: &[],
        minimum: None,
        maximum: None,
        exclusive_minimum: None,
        exclusive_maximum: None,
        multiple_of: None,
        min_items: None,
        max_items: None,
        unique_items: false,
        min_length: None,
        max_length: None,
        pattern: None,
        deprecated: false,
        default: None,
    };

    /// The properties of one field `k` with the given schema and nothing required.
    fn one_property(schema: JsonSchema) -> Properties {
        (
            BTreeMap::from([("k".to_string(), Schema::from(schema))]),
            None,
        )
    }

    fn build(fields: &[FieldSpec]) -> Properties {
        object_properties(&mut SchemaGenerator::new(), fields)
    }

    #[test]
    fn field_without_attributes_keeps_type_schema() {
        let expected: Properties = one_property(base_schema(&mut SchemaGenerator::new()));
        let actual: Properties = build(&[PLAIN]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn field_with_every_attribute_replaces_type_keywords() {
        let expected: Properties = one_property(JsonSchema {
            type_: Some(JsonType::Integer.into()),
            description: Some("Doc".to_string()),
            minimum: Some(serde_json::Number::from(-5_i64)),
            maximum: Some(serde_json::Number::from(18_446_744_073_709_551_615_u64)),
            exclusive_minimum: serde_json::Number::from_f64(0.5),
            exclusive_maximum: serde_json::Number::from_f64(-2.5),
            multiple_of: Some(serde_json::Number::from(7_u64)),
            min_items: Some(10),
            max_items: Some(20),
            unique_items: Some(true),
            min_length: Some(30),
            max_length: Some(40),
            pattern: Some("^a$".to_string()),
            deprecated: Some(true),
            default_value: Some(serde_json::Value::String("d".to_string())),
            ..JsonSchema::default()
        });
        let actual: Properties = build(&[FieldSpec {
            doc: &[" Doc "],
            minimum: Some(NumberLiteral::Signed(-5)),
            maximum: Some(NumberLiteral::Unsigned(u64::MAX)),
            exclusive_minimum: Some(NumberLiteral::Float(0.5)),
            exclusive_maximum: Some(NumberLiteral::Float(-2.5)),
            multiple_of: Some(NumberLiteral::Unsigned(7)),
            min_items: Some(10),
            max_items: Some(20),
            unique_items: true,
            min_length: Some(30),
            max_length: Some(40),
            pattern: Some("^a$"),
            deprecated: true,
            default: Some(DefaultLiteral::String("d")),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_finite_float_attribute_keeps_type_keyword() {
        let expected: Properties = one_property(base_schema(&mut SchemaGenerator::new()));
        let actual: Properties = build(&[FieldSpec {
            minimum: Some(NumberLiteral::Float(f64::INFINITY)),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn blank_doc_keeps_type_description() {
        let expected: Properties = one_property(base_schema(&mut SchemaGenerator::new()));
        let actual: Properties = build(&[FieldSpec {
            doc: &["  ", ""],
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn bool_default_replaces_type_default() {
        let expected: Properties = one_property(JsonSchema {
            default_value: Some(serde_json::Value::Bool(true)),
            ..base_schema(&mut SchemaGenerator::new())
        });
        let actual: Properties = build(&[FieldSpec {
            default: Some(DefaultLiteral::Bool(true)),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_default_replaces_type_default() {
        let expected: Properties = one_property(JsonSchema {
            default_value: Some(serde_json::Value::Number(serde_json::Number::from(-3_i64))),
            ..base_schema(&mut SchemaGenerator::new())
        });
        let actual: Properties = build(&[FieldSpec {
            default: Some(DefaultLiteral::Number(NumberLiteral::Signed(-3))),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn float_default_replaces_type_default() {
        let expected: Properties = one_property(JsonSchema {
            default_value: serde_json::Number::from_f64(2.5).map(serde_json::Value::Number),
            ..base_schema(&mut SchemaGenerator::new())
        });
        let actual: Properties = build(&[FieldSpec {
            default: Some(DefaultLiteral::Number(NumberLiteral::Float(2.5))),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_finite_float_default_removes_type_default() {
        let expected: Properties = one_property(JsonSchema {
            default_value: None,
            ..base_schema(&mut SchemaGenerator::new())
        });
        let actual: Properties = build(&[FieldSpec {
            default: Some(DefaultLiteral::Number(NumberLiteral::Float(f64::NAN))),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_fields_listed_in_field_order() {
        let expected: Option<Vec<String>> = Some(vec!["z".to_string(), "a".to_string()]);
        let actual: Option<Vec<String>> = build(&[
            FieldSpec {
                key: "z",
                required: true,
                ..PLAIN
            },
            FieldSpec { key: "m", ..PLAIN },
            FieldSpec {
                key: "a",
                required: true,
                ..PLAIN
            },
        ])
        .1;
        assert_eq!(expected, actual);
    }

    #[test]
    fn no_fields_build_empty_properties_and_no_required() {
        let expected: Properties = (BTreeMap::new(), None);
        let actual: Properties = build(&[]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn doc_description_trims_lines_and_drops_outer_blank_lines() {
        let expected: Option<String> = Some("First\n\nx\nLast".to_string());
        let actual: Option<String> = doc_description(&["", " First", "", "   x  ", " Last ", " "]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn doc_description_of_no_lines_is_none() {
        let expected: Option<String> = None;
        let actual: Option<String> = doc_description(&[]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn doc_description_of_blank_lines_is_none() {
        let expected: Option<String> = None;
        let actual: Option<String> = doc_description(&[" ", "\t"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn flattened_map_of_any_value_adds_no_additional_properties() {
        let expected: Option<Schema> = None;
        let actual: Option<Schema> = flattened_map_additional_properties(
            &mut SchemaGenerator::new(),
            <serde_json::Value as ToJsonSchema>::json_schema_with,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn flattened_map_of_typed_value_is_value_schema() {
        let expected: Option<Schema> = Some(Schema::from(JsonSchema {
            type_: Some(JsonType::String.into()),
            ..JsonSchema::default()
        }));
        let actual: Option<Schema> = flattened_map_additional_properties(
            &mut SchemaGenerator::new(),
            <String as ToJsonSchema>::json_schema_with,
        );
        assert_eq!(expected, actual);
    }

    type EnumKeywords = (Option<Vec<serde_json::Value>>, Option<serde_json::Value>);

    #[test]
    fn single_value_enum_is_const() {
        let expected: EnumKeywords = (None, Some(serde_json::Value::String("a".to_string())));
        let actual: EnumKeywords = value_enum_keywords(&[EnumValue::Name("a")]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn value_enum_lists_names_and_json_values_in_order() {
        let expected: EnumKeywords = (
            Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::json!({"k": [1, null]}),
            ]),
            None,
        );
        let actual: EnumKeywords = value_enum_keywords(&[
            EnumValue::Name("a"),
            EnumValue::Json(|| serde_json::json!({"k": [1, null]})),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn value_enum_without_variants_is_empty_enum() {
        let expected: EnumKeywords = (Some(Vec::new()), None);
        let actual: EnumKeywords = value_enum_keywords(&[]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn payload_schemas_in_variant_order() {
        let payloads: &[SchemaFn] = &[
            <String as ToJsonSchema>::json_schema_with,
            <bool as ToJsonSchema>::json_schema_with,
        ];
        let expected: Vec<Schema> = vec![
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..JsonSchema::default()
            }),
            Schema::from(JsonSchema {
                type_: Some(JsonType::Boolean.into()),
                ..JsonSchema::default()
            }),
        ];
        let actual: Vec<Schema> = payload_schemas(&mut SchemaGenerator::new(), payloads);
        assert_eq!(expected, actual);
    }
}
