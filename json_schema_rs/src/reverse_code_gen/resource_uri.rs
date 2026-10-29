//! Effective URIs of the schema resources a [`super::SchemaGenerator`] embeds, and the `$ref`
//! from one to another.
//!
//! A derived document's own base URI is unknown unless its root has an absolute `$id`: it is
//! wherever the document is loaded from. So each URI is computed under two **placeholder**
//! document bases that differ in every component (scheme, authority, directories, file name).
//! A reference counts as correct only if it resolves to the target under both, which means it
//! does not depend on the real document base. The placeholder directories are deeper than all
//! the `..` segments the ids can climb, so no resolution is clamped at the root.

use crate::json_schema::registry::{Uri, parse_uri, resolve_uri};

/// A resource's URI under each placeholder document base (equal when it is absolute).
pub(super) type Placed = [Uri; 2];

/// The two placeholder document bases, with `depth` directories each.
pub(super) fn placeholder_bases(depth: usize) -> Placed {
    let base = |scheme: &str, directory: &str| -> Uri {
        let text: String = format!(
            "{scheme}://{scheme}/{}{scheme}.json",
            format!("{directory}/").repeat(depth)
        );
        let Ok(uri) = parse_uri(&text) else {
            unreachable!("a placeholder base is an absolute URI: {text}");
        };
        uri
    };
    [base("json-schema-a", "a"), base("json-schema-b", "b")]
}

/// Directory depth that keeps every `..` of `ids` (resolved through up to `nesting` levels of
/// embedding) inside the placeholder bases.
pub(super) fn placeholder_depth<'a>(ids: impl Iterator<Item = &'a str>, nesting: usize) -> usize {
    let climbs: usize = ids.map(|id| id.matches("..").count()).sum();
    1 + climbs * nesting.max(1)
}

/// Resolves `id` against each of `bases`, without its fragment, or says why `id` is not a URI
/// reference (or cannot be resolved against a base without a hierarchical path).
pub(super) fn place(bases: &Placed, id: &str) -> Result<Placed, String> {
    let resolve = |base: &Uri| -> Result<Uri, String> {
        let resolved: Uri = resolve_uri(base, id)?;
        Ok(resolved.strip_fragment().normalize())
    };
    Ok([resolve(&bases[0])?, resolve(&bases[1])?])
}

/// The `$ref` that a schema at `from` writes to reach the resource at `to`, whose `$id` is
/// `id`: the `$id` as written when it already resolves there (always the case for an absolute
/// `$id`), else the shortest relative reference, else the absolute URI when `to` does not
/// depend on the document base. `None` when no reference resolves to `to` under both
/// placeholder bases (the target depends on more of the document base than `from` does).
pub(super) fn reference(from: &Placed, to: &Placed, id: &str) -> Option<String> {
    let resolves = |candidate: &str| -> bool {
        (0..2).all(|i| {
            resolve_uri(&from[i], candidate)
                .is_ok_and(|resolved| resolved.strip_fragment().normalize() == to[i])
        })
    };
    let candidates: [Option<String>; 5] = [
        Some(id.to_string()),
        relative_path_reference(&from[0], &to[0]),
        Some(path_absolute_reference(&to[0])),
        network_path_reference(&to[0]),
        Some(to[0].as_str().to_string()),
    ];
    candidates.into_iter().flatten().find(|c| resolves(c))
}

/// The relative-path reference from `from` to `to` (the inverse of RFC 3986 §5.2), when both
/// share scheme and authority: `../` for each directory of `from` not shared with `to`, then
/// the rest of `to`'s path and its query.
fn relative_path_reference(from: &Uri, to: &Uri) -> Option<String> {
    let authority = |uri: &Uri| uri.authority().map(|a| a.as_str().to_string());
    if from.scheme().as_str() != to.scheme().as_str() || authority(from) != authority(to) {
        return None;
    }
    let from_path: Vec<&str> = from.path().as_str().split('/').collect();
    let to_path: Vec<&str> = to.path().as_str().split('/').collect();
    let from_directories: &[&str] = &from_path[..from_path.len() - 1];
    let to_directories: &[&str] = &to_path[..to_path.len() - 1];
    let shared: usize = from_directories
        .iter()
        .zip(to_directories)
        .take_while(|(a, b)| a == b)
        .count();
    let mut reference: String = "../".repeat(from_directories.len() - shared);
    reference.push_str(&to_path[shared..].join("/"));
    if reference.is_empty() {
        reference.push_str("./");
    } else if reference
        .split('/')
        .next()
        .is_some_and(|first| first.contains(':'))
    {
        reference.insert_str(0, "./");
    }
    if let Some(query) = to.query() {
        reference.push('?');
        reference.push_str(query.as_str());
    }
    Some(reference)
}

/// `to`'s path and query, as a path-absolute reference.
fn path_absolute_reference(to: &Uri) -> String {
    match to.query() {
        Some(query) => format!("{}?{}", to.path().as_str(), query.as_str()),
        None => to.path().as_str().to_string(),
    }
}

/// `to` without its scheme, as a network-path reference.
fn network_path_reference(to: &Uri) -> Option<String> {
    to.authority()?;
    let scheme_length: usize = to.scheme().as_str().len() + 1;
    Some(to.as_str()[scheme_length..].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(id: &str) -> Placed {
        place(&placeholder_bases(3), id).expect("resolvable id")
    }

    fn reason(id: &str) -> String {
        resolve_uri(&placeholder_bases(1)[0], id).expect_err("not a URI reference")
    }

    #[test]
    fn reference_between_relative_ids_in_the_same_directory_is_the_id() {
        let expected: Option<String> = Some("b.json".to_string());
        let actual: Option<String> = reference(&placed("a.json"), &placed("b.json"), "b.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_from_a_subdirectory_climbs_out_of_it() {
        let expected: Option<String> = Some("../pong.json".to_string());
        let actual: Option<String> =
            reference(&placed("a/ping.json"), &placed("pong.json"), "pong.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_into_a_sibling_directory_climbs_and_descends() {
        let expected: Option<String> = Some("../b/y.json".to_string());
        let actual: Option<String> =
            reference(&placed("a/x.json"), &placed("b/y.json"), "b/y.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_to_an_id_with_parent_segments_resolves_from_the_referrer() {
        let expected: Option<String> = Some("../../../common/address.json".to_string());
        let actual: Option<String> = reference(
            &placed("a/b/x.json"),
            &placed("../common/address.json"),
            "../common/address.json",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_to_an_absolute_id_is_the_id_as_written() {
        let expected: Option<String> = Some("https://example.com/Pong".to_string());
        let actual: Option<String> = reference(
            &placed("a/ping.json"),
            &placed("https://example.com/Pong"),
            "https://example.com/Pong",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_from_an_absolute_id_to_a_relative_id_does_not_exist() {
        let expected: Option<String> = None;
        let actual: Option<String> = reference(
            &placed("https://example.com/ping"),
            &placed("pong.json"),
            "pong.json",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_to_a_path_absolute_id_is_the_id_as_written() {
        let expected: Option<String> = Some("/schemas/node".to_string());
        let actual: Option<String> = reference(
            &placed("a/x.json"),
            &placed("/schemas/node"),
            "/schemas/node",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_from_a_path_absolute_id_to_a_relative_id_does_not_exist() {
        let expected: Option<String> = None;
        let actual: Option<String> =
            reference(&placed("/schemas/node"), &placed("x.json"), "x.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_between_ids_under_one_absolute_base_is_relative() {
        let base: Placed = placed("https://example.com/root/doc.json");
        let from: Placed = place(&base, "a/ping.json").expect("resolvable id");
        let to: Placed = place(&base, "pong.json").expect("resolvable id");
        let expected: Option<String> = Some("../pong.json".to_string());
        let actual: Option<String> = reference(&from, &to, "pong.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_to_a_path_whose_first_segment_has_a_colon_starts_with_dot() {
        let expected: Option<String> = Some("./b:c.json".to_string());
        let actual: Option<String> = relative_path_reference(
            &placed("https://example.com/a.json")[0],
            &placed("https://example.com/b:c.json")[0],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn network_path_reference_drops_the_scheme() {
        let expected: Option<String> = Some("//example.com/x?q".to_string());
        let actual: Option<String> = network_path_reference(&placed("https://example.com/x?q")[0]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn place_rejects_an_id_that_is_not_a_uri_reference() {
        let expected: Result<Placed, String> = Err(reason("not a uri"));
        let actual: Result<Placed, String> = place(&placeholder_bases(1), "not a uri");
        assert_eq!(expected, actual);
    }

    #[test]
    fn placeholder_depth_covers_every_parent_segment_at_every_nesting_level() {
        let expected: usize = 7;
        let actual: usize = placeholder_depth(["../a", "../../b", "c"].into_iter(), 2);
        assert_eq!(expected, actual);
    }
}
