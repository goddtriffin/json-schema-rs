//! Fragment-only `$ref` resolution against a root schema.
//!
//! Supported forms in this crate:
//! - `#` (or empty string) → root schema
//! - `#/$defs/<name>` → lookup in root `$defs`
//! - `#/definitions/<name>` → lookup in root `definitions`
//!
//! Remote references, `$id`-relative resolution, anchors, and full JSON Pointer traversal are out of scope.

use crate::json_schema::JsonSchema;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefResolutionError {
    /// `$ref` is not a fragment-only reference supported by this crate.
    UnsupportedRef { ref_str: String },
    /// Fragment exists but does not match one of the supported paths.
    UnsupportedFragment { ref_str: String },
    /// `$defs` container is missing on the root schema.
    DefsMissing { ref_str: String },
    /// `definitions` container is missing on the root schema.
    DefinitionsMissing { ref_str: String },
    /// The requested key was not found under `$defs`.
    DefNotFound { ref_str: String, name: String },
    /// The requested key was not found under `definitions`.
    DefinitionNotFound { ref_str: String, name: String },
    /// The `$ref` chain contains a cycle.
    RefCycle { ref_str: String },
    /// JSON Pointer escape sequence is invalid.
    InvalidPointerEscape { ref_str: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParsedRef {
    Root,
    Defs(String),
    Definitions(String),
}

fn decode_json_pointer_segment(seg: &str, ref_str: &str) -> Result<String, RefResolutionError> {
    if !seg.contains('~') {
        return Ok(seg.to_string());
    }

    // JSON Pointer: "~1" => "/", "~0" => "~"
    let mut out: String = String::with_capacity(seg.len());
    let mut chars = seg.chars();
    while let Some(c) = chars.next() {
        if c != '~' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('0') => out.push('~'),
            Some('1') => out.push('/'),
            _ => {
                return Err(RefResolutionError::InvalidPointerEscape {
                    ref_str: ref_str.to_string(),
                });
            }
        }
    }
    Ok(out)
}

/// Parses a fragment-only `$ref` string into a [`ParsedRef`].
///
/// # Errors
///
/// Returns [`RefResolutionError`] for non-fragment refs, unsupported fragment paths,
/// invalid JSON Pointer escapes, or malformed segments.
pub fn parse_ref(ref_str: &str) -> Result<ParsedRef, RefResolutionError> {
    if ref_str.is_empty() || ref_str == "#" {
        return Ok(ParsedRef::Root);
    }
    if !ref_str.starts_with('#') {
        return Err(RefResolutionError::UnsupportedRef {
            ref_str: ref_str.to_string(),
        });
    }
    let frag = &ref_str[1..];
    if frag.is_empty() {
        return Ok(ParsedRef::Root);
    }
    if !frag.starts_with('/') {
        return Err(RefResolutionError::UnsupportedFragment {
            ref_str: ref_str.to_string(),
        });
    }

    // Only allow "#/$defs/<name>" and "#/definitions/<name>"
    let mut parts = frag[1..].split('/');
    let container = parts.next().unwrap_or_default();
    let raw_name = parts.next().unwrap_or_default();
    // Must have exactly two segments.
    if container.is_empty() || raw_name.is_empty() || parts.next().is_some() {
        return Err(RefResolutionError::UnsupportedFragment {
            ref_str: ref_str.to_string(),
        });
    }

    let name = decode_json_pointer_segment(raw_name, ref_str)?;
    match container {
        "$defs" => Ok(ParsedRef::Defs(name)),
        "definitions" => Ok(ParsedRef::Definitions(name)),
        _ => Err(RefResolutionError::UnsupportedFragment {
            ref_str: ref_str.to_string(),
        }),
    }
}

/// Resolves a fragment-only `$ref` against the root schema (single step).
///
/// # Errors
///
/// Returns [`RefResolutionError`] when the ref is unsupported, the container is missing,
/// or the definition name is not found.
pub fn resolve_ref<'a>(
    root: &'a JsonSchema,
    ref_str: &str,
) -> Result<&'a JsonSchema, RefResolutionError> {
    match parse_ref(ref_str)? {
        ParsedRef::Root => Ok(root),
        ParsedRef::Defs(name) => {
            let defs = root
                .defs
                .as_ref()
                .ok_or_else(|| RefResolutionError::DefsMissing {
                    ref_str: ref_str.to_string(),
                })?;
            let target = defs
                .get(&name)
                .ok_or_else(|| RefResolutionError::DefNotFound {
                    ref_str: ref_str.to_string(),
                    name,
                })?;
            Ok(target)
        }
        ParsedRef::Definitions(name) => {
            let definitions = root.definitions.as_ref().ok_or_else(|| {
                RefResolutionError::DefinitionsMissing {
                    ref_str: ref_str.to_string(),
                }
            })?;
            let target =
                definitions
                    .get(&name)
                    .ok_or_else(|| RefResolutionError::DefinitionNotFound {
                        ref_str: ref_str.to_string(),
                        name,
                    })?;
            Ok(target)
        }
    }
}

/// Resolves `$ref` on a schema node transitively until the effective schema has no `$ref`.
///
/// Cycle detection is performed on the `$ref` strings encountered.
///
/// # Errors
///
/// Returns [`RefResolutionError`] when any step fails (unsupported ref, missing def, or cycle).
pub fn resolve_schema_ref_transitive<'a>(
    root: &'a JsonSchema,
    schema: &'a JsonSchema,
) -> Result<&'a JsonSchema, RefResolutionError> {
    let mut current: &'a JsonSchema = schema;
    let mut visited: HashSet<&'a str> = HashSet::new();

    while let Some(ref_str) = current.ref_.as_deref() {
        if visited.contains(ref_str) {
            return Err(RefResolutionError::RefCycle {
                ref_str: ref_str.to_string(),
            });
        }
        visited.insert(ref_str);
        current = resolve_ref(root, ref_str)?;
    }

    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedRef, RefResolutionError, parse_ref, resolve_ref, resolve_schema_ref_transitive,
    };
    use crate::json_schema::JsonSchema;

    #[test]
    fn parse_ref_defs() {
        let actual = parse_ref("#/$defs/Foo").unwrap();
        let expected = ParsedRef::Defs("Foo".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_ref_definitions() {
        let actual = parse_ref("#/definitions/Foo").unwrap();
        let expected = ParsedRef::Definitions("Foo".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_ref_root_hash() {
        let actual = parse_ref("#").unwrap();
        let expected = ParsedRef::Root;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_ref_root_empty_string() {
        let actual = parse_ref("").unwrap();
        let expected = ParsedRef::Root;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_ref_unsupported_non_fragment() {
        let actual = parse_ref("http://example.com/schema.json").unwrap_err();
        let expected = RefResolutionError::UnsupportedRef {
            ref_str: "http://example.com/schema.json".to_string(),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_ref_unsupported_extra_segments() {
        let actual = parse_ref("#/$defs/Foo/bar").unwrap_err();
        let expected = RefResolutionError::UnsupportedFragment {
            ref_str: "#/$defs/Foo/bar".to_string(),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_ref_invalid_pointer_escape() {
        let actual = parse_ref("#/$defs/Foo~").unwrap_err();
        let expected = RefResolutionError::InvalidPointerEscape {
            ref_str: "#/$defs/Foo~".to_string(),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_ref_defs_success() {
        let root: JsonSchema = serde_json::from_str(
            r#"{
  "$defs": {
    "Foo": { "type": "string", "title": "FooType" }
  }
}"#,
        )
        .unwrap();
        let actual: &JsonSchema = resolve_ref(&root, "#/$defs/Foo").expect("resolve Foo");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            title: Some("FooType".to_string()),
            ..Default::default()
        };
        assert_eq!(expected, *actual);
    }

    #[test]
    fn resolve_ref_definitions_success() {
        let root: JsonSchema = serde_json::from_str(
            r#"{
  "definitions": {
    "Bar": { "type": "integer", "title": "BarType" }
  }
}"#,
        )
        .unwrap();
        let actual: &JsonSchema = resolve_ref(&root, "#/definitions/Bar").expect("resolve Bar");
        let expected: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            title: Some("BarType".to_string()),
            ..Default::default()
        };
        assert_eq!(expected, *actual);
    }

    #[test]
    fn resolve_ref_root_returns_root() {
        let root: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"x":{"type":"string"}}}"#)
                .unwrap();
        let actual: &JsonSchema = resolve_ref(&root, "#").expect("resolve root");
        let expected: &JsonSchema = &root;
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_ref_decodes_pointer_segment() {
        let root: JsonSchema = serde_json::from_str(
            r#"{
  "$defs": {
    "Foo/bar": { "type": "string" }
  }
}"#,
        )
        .unwrap();
        let actual: &JsonSchema = resolve_ref(&root, "#/$defs/Foo~1bar").expect("resolve Foo/bar");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        assert_eq!(expected, *actual);
    }

    #[test]
    fn resolve_ref_missing_defs_errors() {
        let root: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{}}"#).unwrap();
        let actual = resolve_ref(&root, "#/$defs/Foo").unwrap_err();
        let expected = RefResolutionError::DefsMissing {
            ref_str: "#/$defs/Foo".to_string(),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_ref_not_found_errors() {
        let root: JsonSchema = serde_json::from_str(
            r#"{"$defs":{"Bar":{"type":"string"}},"type":"object","properties":{}}"#,
        )
        .unwrap();
        let actual = resolve_ref(&root, "#/$defs/Foo").unwrap_err();
        let expected = RefResolutionError::DefNotFound {
            ref_str: "#/$defs/Foo".to_string(),
            name: "Foo".to_string(),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_schema_ref_transitive_cycle_errors() {
        let root: JsonSchema = serde_json::from_str(
            r##"{
  "$defs": {
    "A": { "$ref": "#/$defs/B" },
    "B": { "$ref": "#/$defs/A" }
  },
  "$ref": "#/$defs/A"
}"##,
        )
        .unwrap();
        let actual = resolve_schema_ref_transitive(&root, &root).unwrap_err();
        let expected = RefResolutionError::RefCycle {
            ref_str: "#/$defs/A".to_string(),
        };
        assert_eq!(expected, actual);
    }
}
