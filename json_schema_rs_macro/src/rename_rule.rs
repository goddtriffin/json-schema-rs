//! Reading serde's container `#[serde(rename_all = "...")]`; the case conversion itself is
//! [`json_schema_rs::sanitizers::RenameRule`], shared with codegen.

use json_schema_rs::sanitizers::RenameRule;
use syn::{Attribute, Error, LitStr, Result as SynResult, Token};

/// The rule of the container's `#[serde(rename_all = "...")]`, if it has one; an unknown rule
/// is an error at its literal, with serde's list of the valid ones.
///
/// The other arguments of the container's serde attributes are skipped whatever their form
/// (serde validates them).
pub fn rename_rule_from_attrs(attrs: &[Attribute]) -> SynResult<Option<RenameRule>> {
    let mut rule: Option<RenameRule> = None;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") && meta.input.peek(Token![=]) {
                let lit: LitStr = meta.value()?.parse()?;
                rule = Some(rename_rule_from_lit(&lit)?);
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

/// The rule spelled by `lit`, or an error at `lit`.
pub fn rename_rule_from_lit(lit: &LitStr) -> SynResult<RenameRule> {
    let value: String = lit.value();
    RenameRule::from_serde_name(&value).ok_or_else(|| {
        let names: Vec<String> = RenameRule::ALL
            .iter()
            .map(|rule: &RenameRule| format!("\"{}\"", rule.serde_name()))
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

#[cfg(test)]
mod tests {
    use super::rename_rule_from_attrs;
    use json_schema_rs::sanitizers::RenameRule;
    use proc_macro2::TokenStream;
    use std::str::FromStr;
    use syn::DeriveInput;

    /// The rule the container attributes of `source` give.
    fn rule_of(source: &str) -> syn::Result<Option<RenameRule>> {
        let tokens: TokenStream = TokenStream::from_str(source).expect("tokenize");
        let input: DeriveInput = syn::parse2(tokens).expect("parse");
        rename_rule_from_attrs(&input.attrs)
    }

    #[test]
    fn container_rename_all_is_read_among_other_arguments() {
        let expected: Option<RenameRule> = Some(RenameRule::Camel);
        let actual: Option<RenameRule> = rule_of(
            "#[serde(deny_unknown_fields, rename_all = \"camelCase\", bound(serialize = \"\"))] struct S;",
        )
        .expect("parses");
        assert_eq!(expected, actual);
    }

    #[test]
    fn container_without_rename_all_has_no_rule() {
        let expected: Option<RenameRule> = None;
        let actual: Option<RenameRule> =
            rule_of("#[serde(deny_unknown_fields, rename = \"T\")] struct S;").expect("parses");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_rule_is_an_error_at_its_literal() {
        let expected: Option<(String, usize)> = Some((
            "unknown rename rule `rename_all = \"Title Case\"`, expected one of \"lowercase\", \"UPPERCASE\", \"PascalCase\", \"camelCase\", \"snake_case\", \"SCREAMING_SNAKE_CASE\", \"kebab-case\", \"SCREAMING-KEBAB-CASE\"".to_string(),
            21,
        ));
        let actual: Option<(String, usize)> =
            rule_of("#[serde(rename_all = \"Title Case\")] struct S;")
                .err()
                .map(|error| (error.to_string(), error.span().start().column));
        assert_eq!(expected, actual);
    }
}
