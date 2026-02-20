//! Sanitization of names and paths for Rust codegen and CLI output.
//!
//! All functions that produce valid Rust identifiers (struct names, field names,
//! module names, file/directory path components) live here as a single source of truth.

use std::path::{Path, PathBuf};

/// Sanitize a JSON property key to a Rust field identifier (`snake_case`; replace `-` with `_`).
/// Does not change case; only replaces invalid characters. Result is safe for use as a field name.
/// Empty input becomes `"empty"`; leading digit becomes `field_{s}`.
#[must_use]
pub fn sanitize_field_name(key: &str) -> String {
    let s: String = key
        .chars()
        .map(|c| if c == '-' { '_' } else { c })
        .collect();
    if s.is_empty() {
        return "empty".to_string();
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("field_{s}");
    }
    if s.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()) {
        return s;
    }
    s.chars()
        .map(|c| {
            if c == '_' || c.is_ascii_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Convert a name to `PascalCase` for struct or enum names (e.g. "address" -> "Address").
/// Splits on `_`, `-`, and space; capitalizes each word. Empty becomes `"Unnamed"`;
/// leading digit becomes `N{out}`.
#[must_use]
pub fn to_pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for c in name.chars() {
        if c == '_' || c == '-' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            out.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "Unnamed".to_string()
    } else if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("N{out}")
    } else {
        out
    }
}

/// Ensure a struct (or enum) name is a valid Rust type identifier (`PascalCase`; prefix if starts with digit).
#[must_use]
pub fn sanitize_struct_name(s: &str) -> String {
    let pascal = to_pascal_case(s);
    if pascal.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("N{pascal}")
    } else {
        pascal
    }
}

/// Maps a path component (file stem or dir name) to a Rust-valid identifier.
/// Replaces `-` with `_` and any character not in `[a-zA-Z0-9_]` with `_`.
/// Empty becomes `"schema"`; leading digit becomes `_{s}`.
#[must_use]
pub fn sanitize_path_component(component: &str) -> String {
    let s: String = component
        .chars()
        .map(|c| {
            if c == '-' || c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .replace('-', "_");
    if s.is_empty() {
        return "schema".to_string();
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("_{s}");
    }
    s
}

/// Builds the sanitized output relative path (e.g. `sub_dir/schema_2.rs`) from a relative path (e.g. `sub-dir/schema-2.json`).
#[must_use]
pub fn sanitize_output_relative(relative: &Path) -> PathBuf {
    let components: Vec<_> = relative.components().collect();
    let mut out = PathBuf::new();
    for (i, comp) in components.iter().enumerate() {
        let os = comp.as_os_str();
        let s = os.to_string_lossy();
        let is_last = i == components.len() - 1;
        if is_last {
            let stem = Path::new(s.as_ref())
                .file_stem()
                .and_then(|st| st.to_str())
                .unwrap_or("schema");
            let name = format!("{}.rs", sanitize_path_component(stem));
            out.push(name);
        } else {
            out.push(sanitize_path_component(s.as_ref()));
        }
    }
    out
}

/// Sanitize a string to a valid Rust module name (`snake_case`, no leading digit).
/// Replaces `-`, `.`, space with `_`; keeps only alphanumeric and `_`. Empty becomes `"schema"`;
/// leading digit becomes `schema_{s}`; reserved `crate`/`self`/`super` become `{s}_mod`.
#[must_use]
pub fn sanitize_module_name(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| {
            if c == '-' || c == '.' || c == ' ' {
                '_'
            } else {
                c
            }
        })
        .filter(|c| *c == '_' || c.is_ascii_alphanumeric())
        .collect();
    if s.is_empty() {
        return "schema".to_string();
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("schema_{s}");
    }
    if s == "crate" || s == "self" || s == "super" {
        return format!("{s}_mod");
    }
    s
}

/// Module name from a file path: takes the file stem then applies [`sanitize_module_name`].
#[must_use]
pub fn module_name_from_path(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("schema");
    sanitize_module_name(stem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn sanitize_field_name_replaces_hyphen() {
        let expected = "foo_bar";
        let actual = sanitize_field_name("foo-bar");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_unchanged_valid() {
        let expected = "first_name";
        let actual = sanitize_field_name("first_name");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_address() {
        let expected = "Address";
        let actual = to_pascal_case("address");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_street_address() {
        let expected = "StreetAddress";
        let actual = to_pascal_case("street_address");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_hyphen_to_underscore() {
        let expected = "schema_1";
        let actual = sanitize_path_component("schema-1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_unchanged_valid() {
        let expected = "sub_dir";
        let actual = sanitize_path_component("sub_dir");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_empty_fallback() {
        let expected = "schema";
        let actual = sanitize_path_component("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_only_hyphens_becomes_underscores() {
        let expected = "___";
        let actual = sanitize_path_component("---");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_starts_with_digit_prefixed() {
        let expected = "_123";
        let actual = sanitize_path_component("123");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_output_relative_single_file() {
        let relative = Path::new("schema-1.json");
        let actual = sanitize_output_relative(relative);
        let expected = PathBuf::from("schema_1.rs");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_output_relative_nested() {
        let relative = Path::new("sub-dir/schema-2.json");
        let actual = sanitize_output_relative(relative);
        let expected = PathBuf::from("sub_dir/schema_2.rs");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_module_name_empty() {
        let expected = "schema";
        let actual = sanitize_module_name("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_module_name_leading_digit() {
        let expected = "schema_123";
        let actual = sanitize_module_name("123");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_module_name_reserved_crate() {
        let expected = "crate_mod";
        let actual = sanitize_module_name("crate");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_module_name_reserved_self() {
        let expected = "self_mod";
        let actual = sanitize_module_name("self");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_module_name_reserved_super() {
        let expected = "super_mod";
        let actual = sanitize_module_name("super");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_module_name_normal_stem_unchanged() {
        let expected = "my_schema";
        let actual = sanitize_module_name("my_schema");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_module_name_hyphen_to_underscore() {
        let expected = "my_schema";
        let actual = sanitize_module_name("my-schema");
        assert_eq!(expected, actual);
    }

    #[test]
    fn module_name_from_path_uses_stem() {
        let expected = "schema_1";
        let actual = module_name_from_path("dir/schema-1.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn module_name_from_path_no_extension() {
        let expected = "schema";
        let actual = module_name_from_path("path/to/schema");
        assert_eq!(expected, actual);
    }
}
