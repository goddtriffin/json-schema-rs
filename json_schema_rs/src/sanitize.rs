//! Sanitization of names and paths for Rust codegen and CLI output.
//!
//! All functions that produce valid Rust identifiers (struct names, field names,
//! module names, file/directory path components) live here as a single source of truth.
//! Non-ASCII characters are replaced with `_`. Rust strict and reserved keywords
//! are escaped with a trailing `_` so output is always valid.

use std::path::{Path, PathBuf};

/// Replace any non-ASCII character with `_`. Used at the start of sanitizers so
/// the rest of the logic sees only ASCII and output is stable.
#[must_use]
fn replace_non_ascii(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii() { c } else { '_' })
        .collect()
}

/// Rust strict and reserved keywords (lowercase) that cannot be used as field/variable identifiers.
/// Source: <https://doc.rust-lang.org/reference/keywords.html>
const RUST_KEYWORDS_FIELD: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "gen", "macro", "override",
    "priv", "try", "typeof", "unsized", "virtual", "yield",
];

/// The only `PascalCase` keyword in Rust is `Self`. Used for type/struct names.
const RUST_KEYWORD_TYPE_SELF: &str = "Self";

fn is_rust_keyword_field(s: &str) -> bool {
    RUST_KEYWORDS_FIELD.contains(&s)
}

/// Sanitize a JSON property key to a Rust field identifier (`snake_case`; replace `-` with `_`).
/// Does not change case; only replaces invalid characters. Result is safe for use as a field name.
/// Empty input becomes `"empty"`; leading digit becomes `field_{s}`; single `_` becomes `"empty"`;
/// Rust keywords get a trailing `_`. Non-ASCII is replaced with `_`.
#[must_use]
pub fn sanitize_field_name(key: &str) -> String {
    let key = replace_non_ascii(key);
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
    let s: String = if s.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()) {
        s
    } else {
        s.chars()
            .map(|c| {
                if c == '_' || c.is_ascii_alphanumeric() {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    if s == "_" {
        return "empty".to_string();
    }
    if is_rust_keyword_field(&s) {
        return format!("{s}_");
    }
    s
}

/// Convert a name to `PascalCase` for struct or enum names (e.g. "address" -> "Address").
/// Splits on `_`, `-`, and space; capitalizes each word. Empty becomes `"Unnamed"`;
/// leading digit becomes `N{out}`. Non-ASCII is replaced with `_` before conversion.
#[must_use]
pub fn to_pascal_case(name: &str) -> String {
    let name = replace_non_ascii(name);
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
/// Rust keyword `Self` (the only `PascalCase` keyword) is escaped as `Self_`. Non-ASCII is replaced in [`to_pascal_case`].
#[must_use]
pub fn sanitize_struct_name(s: &str) -> String {
    let pascal = to_pascal_case(s);
    let pascal = if pascal.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("N{pascal}")
    } else {
        pascal
    };
    if pascal == RUST_KEYWORD_TYPE_SELF {
        format!("{pascal}_")
    } else {
        pascal
    }
}

/// Maps a path component (file stem or dir name) to a Rust-valid identifier.
/// Replaces `-` with `_` and any character not in `[a-zA-Z0-9_]` with `_`.
/// Empty becomes `"schema"`; leading digit becomes `_{s}`. Non-ASCII is replaced with `_`.
#[must_use]
pub fn sanitize_path_component(component: &str) -> String {
    let component = replace_non_ascii(component);
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
/// leading digit becomes `schema_{s}`; reserved `crate`/`self`/`super` become `{s}_mod`. Non-ASCII is replaced with `_`.
#[must_use]
pub fn sanitize_module_name(s: &str) -> String {
    let s = replace_non_ascii(s);
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
    fn to_pascal_case_empty() {
        let expected = "Unnamed";
        let actual = to_pascal_case("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_leading_digit() {
        let expected = "N123";
        let actual = to_pascal_case("123");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_consecutive_underscores() {
        let expected = "FooBar";
        let actual = to_pascal_case("foo__bar");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_consecutive_hyphens() {
        let expected = "FooBar";
        let actual = to_pascal_case("foo--bar");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_consecutive_spaces() {
        let expected = "FooBar";
        let actual = to_pascal_case("foo  bar");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_mixed_separators() {
        let expected = "StreetAddress";
        let actual = to_pascal_case("street - address");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_single_char() {
        let expected = "A";
        let actual = to_pascal_case("a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_already_pascal_case() {
        let expected = "Address";
        let actual = to_pascal_case("Address");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_all_separators_unnamed() {
        let expected = "Unnamed";
        let actual = to_pascal_case("  __ --  ");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_multiple_words() {
        let expected = "MySchemaType";
        let actual = to_pascal_case("my_schema_type");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_non_ascii_replaced() {
        let expected = "Caf";
        let actual = to_pascal_case("café");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_struct_name_self_keyword_escaped() {
        let expected = "Self_";
        let actual = sanitize_struct_name("self");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_struct_name_type_not_keyword_in_pascal() {
        let expected = "Type";
        let actual = sanitize_struct_name("type");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_struct_name_struct_not_keyword_in_pascal() {
        let expected = "Struct";
        let actual = sanitize_struct_name("struct");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_struct_name_leading_digit() {
        let expected = "N123";
        let actual = sanitize_struct_name("123");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_struct_name_empty_unnamed() {
        let expected = "Unnamed";
        let actual = sanitize_struct_name("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_empty() {
        let expected = "empty";
        let actual = sanitize_field_name("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_leading_digit() {
        let expected = "field_123";
        let actual = sanitize_field_name("123");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_keyword_type_escaped() {
        let expected = "type_";
        let actual = sanitize_field_name("type");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_keyword_self_escaped() {
        let expected = "self_";
        let actual = sanitize_field_name("self");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_single_underscore_becomes_empty() {
        let expected = "empty";
        let actual = sanitize_field_name("_");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_invalid_chars_replaced() {
        let expected = "foo_bar_baz";
        let actual = sanitize_field_name("foo.bar%baz");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_non_ascii_replaced() {
        let expected = "empty";
        let actual = sanitize_field_name("é");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_keyword_async_escaped() {
        let expected = "async_";
        let actual = sanitize_field_name("async");
        assert_eq!(expected, actual);
    }

    #[test]
    fn stability_golden_type_struct_name() {
        let expected = "Type";
        let actual = sanitize_struct_name("type");
        assert_eq!(expected, actual);
    }

    #[test]
    fn stability_golden_type_field_name() {
        let expected = "type_";
        let actual = sanitize_field_name("type");
        assert_eq!(expected, actual);
    }

    #[test]
    fn stability_golden_self_struct_name() {
        let expected = "Self_";
        let actual = sanitize_struct_name("self");
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
