//! Serde's container `#[serde(rename_all = "...")]` case conversion, exactly as serde applies it.

use syn::{Attribute, Error, LitStr, Result as SynResult, Token};

/// A case rule of `#[serde(rename_all = "...")]`: how serde names the members that have no
/// `rename` of their own.
///
/// Fields are converted from their `snake_case` Rust names, variants from their `PascalCase`
/// names, character for character as serde's derive does (ASCII case mapping only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameRule {
    Lower,
    Upper,
    Pascal,
    Camel,
    Snake,
    ScreamingSnake,
    Kebab,
    ScreamingKebab,
}

/// Every rule with its serde spelling, in serde's order.
const RULES: [(&str, RenameRule); 8] = [
    ("lowercase", RenameRule::Lower),
    ("UPPERCASE", RenameRule::Upper),
    ("PascalCase", RenameRule::Pascal),
    ("camelCase", RenameRule::Camel),
    ("snake_case", RenameRule::Snake),
    ("SCREAMING_SNAKE_CASE", RenameRule::ScreamingSnake),
    ("kebab-case", RenameRule::Kebab),
    ("SCREAMING-KEBAB-CASE", RenameRule::ScreamingKebab),
];

impl RenameRule {
    /// The rule of the container's `#[serde(rename_all = "...")]`, if it has one; an unknown
    /// rule is an error at its literal, with serde's list of the valid ones.
    ///
    /// The other arguments of the container's serde attributes are skipped whatever their form
    /// (serde validates them).
    pub fn from_attrs(attrs: &[Attribute]) -> SynResult<Option<Self>> {
        let mut rule: Option<Self> = None;
        for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename_all") && meta.input.peek(Token![=]) {
                    let lit: LitStr = meta.value()?.parse()?;
                    rule = Some(Self::from_str(&lit)?);
                } else if meta.input.peek(Token![=]) {
                    let _: syn::Expr = meta.value()?.parse()?;
                } else if meta.input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    content.parse::<proc_macro2::TokenStream>()?;
                }
                Ok(())
            })?;
        }
        Ok(rule)
    }

    /// The rule spelled `lit`, or an error at `lit`.
    fn from_str(lit: &LitStr) -> SynResult<Self> {
        let value: String = lit.value();
        RULES
            .iter()
            .find(|(name, _)| *name == value)
            .map(|(_, rule)| *rule)
            .ok_or_else(|| {
                let names: Vec<String> = RULES
                    .iter()
                    .map(|(name, _)| format!("\"{name}\""))
                    .collect();
                Error::new(
                    lit.span(),
                    format!(
                        "unknown rename rule `rename_all = \"{value}\"`, expected one of {}",
                        names.join(", ")
                    ),
                )
            })
    }

    /// The serde name of a field named `field` (`snake_case`, without `r#`).
    pub fn apply_to_field(self, field: &str) -> String {
        match self {
            Self::Lower | Self::Snake => field.to_owned(),
            Self::Upper | Self::ScreamingSnake => field.to_ascii_uppercase(),
            Self::Pascal => {
                let mut pascal: String = String::with_capacity(field.len());
                let mut capitalize: bool = true;
                for ch in field.chars() {
                    if ch == '_' {
                        capitalize = true;
                    } else if capitalize {
                        pascal.push(ch.to_ascii_uppercase());
                        capitalize = false;
                    } else {
                        pascal.push(ch);
                    }
                }
                pascal
            }
            Self::Camel => lower_first(&Self::Pascal.apply_to_field(field)),
            Self::Kebab => field.replace('_', "-"),
            Self::ScreamingKebab => field.to_ascii_uppercase().replace('_', "-"),
        }
    }

    /// The serde name of a variant named `variant` (`PascalCase`).
    pub fn apply_to_variant(self, variant: &str) -> String {
        match self {
            Self::Pascal => variant.to_owned(),
            Self::Lower => variant.to_ascii_lowercase(),
            Self::Upper => variant.to_ascii_uppercase(),
            Self::Camel => lower_first(variant),
            Self::Snake => {
                let mut snake: String = String::with_capacity(variant.len() + 4);
                for (i, ch) in variant.char_indices() {
                    if i > 0 && ch.is_uppercase() {
                        snake.push('_');
                    }
                    snake.push(ch.to_ascii_lowercase());
                }
                snake
            }
            Self::ScreamingSnake => Self::Snake.apply_to_variant(variant).to_ascii_uppercase(),
            Self::Kebab => Self::Snake.apply_to_variant(variant).replace('_', "-"),
            Self::ScreamingKebab => Self::ScreamingSnake
                .apply_to_variant(variant)
                .replace('_', "-"),
        }
    }
}

/// `name` with its first character in ASCII lowercase.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first: char| {
        first.to_ascii_lowercase().to_string() + chars.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::RenameRule;
    use proc_macro2::TokenStream;
    use std::str::FromStr;
    use syn::DeriveInput;

    /// The rule each serde spelling names, applied to the field `user_id_v2` and the variant
    /// `UserIdV2`.
    fn applied(rule: &str) -> (String, String) {
        let source: String = format!("#[serde(rename_all = \"{rule}\")] struct S;");
        let tokens: TokenStream = TokenStream::from_str(&source).expect("tokenize");
        let input: DeriveInput = syn::parse2(tokens).expect("parse");
        let rule: RenameRule = RenameRule::from_attrs(&input.attrs)
            .expect("known rule")
            .expect("rule present");
        (
            rule.apply_to_field("user_id_v2"),
            rule.apply_to_variant("UserIdV2"),
        )
    }

    #[test]
    fn lowercase_rule() {
        let expected: (String, String) = ("user_id_v2".to_string(), "useridv2".to_string());
        let actual: (String, String) = applied("lowercase");
        assert_eq!(expected, actual);
    }

    #[test]
    fn uppercase_rule() {
        let expected: (String, String) = ("USER_ID_V2".to_string(), "USERIDV2".to_string());
        let actual: (String, String) = applied("UPPERCASE");
        assert_eq!(expected, actual);
    }

    #[test]
    fn pascal_case_rule() {
        let expected: (String, String) = ("UserIdV2".to_string(), "UserIdV2".to_string());
        let actual: (String, String) = applied("PascalCase");
        assert_eq!(expected, actual);
    }

    #[test]
    fn camel_case_rule() {
        let expected: (String, String) = ("userIdV2".to_string(), "userIdV2".to_string());
        let actual: (String, String) = applied("camelCase");
        assert_eq!(expected, actual);
    }

    #[test]
    fn snake_case_rule() {
        let expected: (String, String) = ("user_id_v2".to_string(), "user_id_v2".to_string());
        let actual: (String, String) = applied("snake_case");
        assert_eq!(expected, actual);
    }

    #[test]
    fn screaming_snake_case_rule() {
        let expected: (String, String) = ("USER_ID_V2".to_string(), "USER_ID_V2".to_string());
        let actual: (String, String) = applied("SCREAMING_SNAKE_CASE");
        assert_eq!(expected, actual);
    }

    #[test]
    fn kebab_case_rule() {
        let expected: (String, String) = ("user-id-v2".to_string(), "user-id-v2".to_string());
        let actual: (String, String) = applied("kebab-case");
        assert_eq!(expected, actual);
    }

    #[test]
    fn screaming_kebab_case_rule() {
        let expected: (String, String) = ("USER-ID-V2".to_string(), "USER-ID-V2".to_string());
        let actual: (String, String) = applied("SCREAMING-KEBAB-CASE");
        assert_eq!(expected, actual);
    }

    #[test]
    fn container_without_rename_all_has_no_rule() {
        let tokens: TokenStream =
            TokenStream::from_str("#[serde(deny_unknown_fields, rename = \"T\")] struct S;")
                .expect("tokenize");
        let input: DeriveInput = syn::parse2(tokens).expect("parse");
        let expected: Option<RenameRule> = None;
        let actual: Option<RenameRule> = RenameRule::from_attrs(&input.attrs).expect("parses");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_rule_is_an_error_at_its_literal() {
        let tokens: TokenStream =
            TokenStream::from_str("#[serde(rename_all = \"Title Case\")] struct S;")
                .expect("tokenize");
        let input: DeriveInput = syn::parse2(tokens).expect("parse");
        let expected: Option<(String, usize)> = Some((
            "unknown rename rule `rename_all = \"Title Case\"`, expected one of \"lowercase\", \"UPPERCASE\", \"PascalCase\", \"camelCase\", \"snake_case\", \"SCREAMING_SNAKE_CASE\", \"kebab-case\", \"SCREAMING-KEBAB-CASE\"".to_string(),
            21,
        ));
        let actual: Option<(String, usize)> = RenameRule::from_attrs(&input.attrs)
            .err()
            .map(|error| (error.to_string(), error.span().start().column));
        assert_eq!(expected, actual);
    }
}
