//! Checking a schema document against its draft's metaschema.
//!
//! The metaschema of each draft (4 to 2020-12) is embedded and compiled once, on first use.
//! [`JsonSchemaSettings::metaschema_check`](crate::JsonSchemaSettings::metaschema_check) runs the
//! same check while parsing.
//!
//! # Example
//!
//! ```
//! use json_schema_rs::meta;
//! use serde_json::json;
//!
//! assert!(meta::is_valid(&json!({"type": "string", "minLength": 1})));
//! // The 2020-12 metaschema requires the items of `required` to be unique.
//! let errors = meta::validate(&json!({"required": ["a", "a"]})).unwrap_err();
//! assert_eq!("/required: array has duplicate items (value: \"a\")", errors[0].to_string());
//! ```

use std::sync::OnceLock;

use serde_json::Value;

use crate::json_schema::registry::METASCHEMAS;
use crate::json_schema::{JsonSchemaSettings, Registry, Schema, SpecVersion};
use crate::validator::{CompiledSchema, ValidationResult};

/// Validates `schema` against the metaschema of the draft its `$schema` names, or of Draft
/// 2020-12 when `$schema` is absent or names no draft from 4 to 2020-12. Every failure is listed;
/// its instance location points into `schema`.
///
/// # Errors
///
/// One [`ValidationError`](crate::ValidationError) per way `schema` does not conform.
pub fn validate(schema: &Value) -> ValidationResult {
    metaschema(draft_of(schema)).validate(schema)
}

/// Whether `schema` conforms to its draft's metaschema (see [`validate`]), stopping at the first
/// failure.
#[must_use]
pub fn is_valid(schema: &Value) -> bool {
    metaschema(draft_of(schema)).is_valid(schema)
}

/// The draft whose metaschema checks `schema`.
fn draft_of(schema: &Value) -> SpecVersion {
    schema
        .get("$schema")
        .and_then(Value::as_str)
        .and_then(SpecVersion::from_schema_uri)
        .filter(|version| has_metaschema(*version))
        .unwrap_or(SpecVersion::Draft202012)
}

/// Whether the metaschema of `version` is embedded: drafts 4 (and 5, its editorial revision) to
/// 2020-12.
pub(crate) fn has_metaschema(version: SpecVersion) -> bool {
    !matches!(
        version,
        SpecVersion::Draft00 | SpecVersion::Draft01 | SpecVersion::Draft02 | SpecVersion::Draft03
    )
}

/// The compiled metaschema of `version` (draft 4's for drafts 0 to 5).
pub(crate) fn metaschema(version: SpecVersion) -> &'static CompiledSchema {
    static COMPILED: [OnceLock<CompiledSchema>; 5] = [const { OnceLock::new() }; 5];
    let (slot, uri): (usize, &str) = match version {
        SpecVersion::Draft00
        | SpecVersion::Draft01
        | SpecVersion::Draft02
        | SpecVersion::Draft03
        | SpecVersion::Draft04
        | SpecVersion::Draft05 => (0, "http://json-schema.org/draft-04/schema"),
        SpecVersion::Draft06 => (1, "http://json-schema.org/draft-06/schema"),
        SpecVersion::Draft07 => (2, "http://json-schema.org/draft-07/schema"),
        SpecVersion::Draft201909 => (3, "https://json-schema.org/draft/2019-09/schema"),
        SpecVersion::Draft202012 => (4, "https://json-schema.org/draft/2020-12/schema"),
    };
    COMPILED[slot].get_or_init(|| compile(uri))
}

/// Compiles the embedded metaschema at `uri`. The embedded documents are fixed, so a failure is a
/// bug in this library.
fn compile(uri: &str) -> CompiledSchema {
    let text: &str = METASCHEMAS
        .iter()
        .find(|(known, _)| *known == uri)
        .map(|(_, text)| *text)
        .expect("every draft's metaschema is embedded");
    let schema: Schema = Schema::new_from_str(text, &JsonSchemaSettings::default())
        .expect("the embedded metaschema parses");
    let registry: Registry<'_> = Registry::new(&schema).expect("the embedded metaschema resolves");
    CompiledSchema::new(&registry).expect("the embedded metaschema compiles")
}

#[cfg(test)]
mod tests {
    use super::{is_valid, validate};
    use crate::json_schema::registry::METASCHEMAS;
    use crate::validator::{ValidationError, ValidationResult};
    use serde_json::{Value, json};

    #[test]
    fn every_embedded_metaschema_conforms_to_its_own_draft() {
        let expected: Vec<(&str, bool)> = METASCHEMAS.iter().map(|(uri, _)| (*uri, true)).collect();
        let actual: Vec<(&str, bool)> = METASCHEMAS
            .iter()
            .map(|(uri, text)| {
                let document: Value = serde_json::from_str(text).expect("embedded JSON");
                (*uri, validate(&document).is_ok())
            })
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_gives_every_error_with_its_metaschema_location() {
        let expected: Result<(), Vec<(String, String, Option<String>)>> = Err(vec![(
            "/minLength".to_string(),
            "/allOf/3/$ref/properties/minLength/$ref/$ref/minimum".to_string(),
            Some(
                "https://json-schema.org/draft/2020-12/meta/validation#/$defs/nonNegativeInteger/minimum"
                    .to_string(),
            ),
        )]);
        let actual: Result<(), Vec<(String, String, Option<String>)>> =
            validate(&json!({"minLength": -1})).map_err(|errors| {
                errors
                    .iter()
                    .map(|e: &ValidationError| {
                        (
                            e.instance_location.to_string(),
                            e.keyword_location.to_string(),
                            e.absolute_keyword_location.clone(),
                        )
                    })
                    .collect()
            });
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_uses_the_draft_schema_names() {
        let expected: bool = false;
        let actual: bool = validate(&json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "required": []
        }))
        .is_ok();
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_uses_2020_12_without_schema() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(&json!({"required": []}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_uses_2020_12_for_an_unrecognised_schema() {
        let expected: bool = false;
        let actual: bool = validate(&json!({
            "$schema": "https://example.com/custom",
            "required": ["a", "a"]
        }))
        .is_ok();
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_uses_2020_12_for_a_draft_without_an_embedded_metaschema() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(&json!({
            "$schema": "http://json-schema.org/draft-03/schema#",
            "required": []
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_rejects_a_schema_that_is_not_an_object_or_boolean() {
        let expected: bool = false;
        let actual: bool = validate(&json!(5)).is_ok();
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_valid_accepts_a_conforming_schema() {
        let expected: bool = true;
        let actual: bool = is_valid(&json!({"type": "string"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_valid_rejects_a_nonconforming_schema() {
        let expected: bool = false;
        let actual: bool = is_valid(&json!({"type": "strng"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_and_is_valid_agree_for_a_draft_7_schema() {
        let schema: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "items": [{"type": "string"}],
            "definitions": {"a": {}}
        });
        let expected: (bool, bool) = (true, true);
        let actual: (bool, bool) = (validate(&schema).is_ok(), is_valid(&schema));
        assert_eq!(expected, actual);
    }
}
