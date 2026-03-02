//! JSON Schema specification version.

use super::settings::JsonSchemaSettings;

/// JSON Schema specification version. One variant per vendored spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecVersion {
    Draft00,
    Draft01,
    Draft02,
    Draft03,
    Draft04,
    Draft05,
    Draft06,
    Draft07,
    Draft201909,
    Draft202012,
}

impl SpecVersion {
    /// Returns the canonical meta-schema URI for this draft (e.g. for use in `$schema`).
    ///
    /// URIs match the local specs (draft-00 through 2020-12). Drafts 00–02 used
    /// hyper-schema URIs; draft-03 onward use schema# or draft/YYYY-MM/schema.
    #[must_use]
    pub fn schema_uri(self) -> &'static str {
        match self {
            SpecVersion::Draft00 => "http://json-schema.org/draft-00/hyper-schema#",
            SpecVersion::Draft01 => "http://json-schema.org/draft-01/hyper-schema#",
            SpecVersion::Draft02 => "http://json-schema.org/draft-02/hyper-schema#",
            SpecVersion::Draft03 => "http://json-schema.org/draft-03/schema#",
            SpecVersion::Draft04 => "http://json-schema.org/draft-04/schema#",
            SpecVersion::Draft05 => "http://json-schema.org/draft-05/schema#",
            SpecVersion::Draft06 => "http://json-schema.org/draft-06/schema#",
            SpecVersion::Draft07 => "http://json-schema.org/draft-07/schema#",
            SpecVersion::Draft201909 => "https://json-schema.org/draft/2019-09/schema",
            SpecVersion::Draft202012 => "https://json-schema.org/draft/2020-12/schema",
        }
    }

    /// Parses a `$schema` URI string and returns the corresponding [`SpecVersion`], if recognized.
    ///
    /// Matching is done by comparing the trimmed string to canonical URIs (with or without
    /// trailing slash). The legacy draft-04 URI `http://json-schema.org/schema#` is
    /// deprecated and returns [`Some(SpecVersion::Draft04)`] for compatibility.
    ///
    /// Returns `None` for empty, unknown, or malformed URIs.
    #[must_use]
    pub fn from_schema_uri(s: &str) -> Option<SpecVersion> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        // Normalize: strip trailing slash for comparison (canonical URIs have no trailing slash except hyper-schema#)
        let s_normalized = s.trim_end_matches('/');
        match s_normalized {
            "http://json-schema.org/draft-00/hyper-schema#" => Some(SpecVersion::Draft00),
            "http://json-schema.org/draft-01/hyper-schema#" => Some(SpecVersion::Draft01),
            "http://json-schema.org/draft-02/hyper-schema#" => Some(SpecVersion::Draft02),
            "http://json-schema.org/draft-03/schema#" => Some(SpecVersion::Draft03),
            "http://json-schema.org/draft-04/schema#" | "http://json-schema.org/schema#" => {
                Some(SpecVersion::Draft04)
            } // second is legacy deprecated
            "http://json-schema.org/draft-05/schema#" => Some(SpecVersion::Draft05),
            "http://json-schema.org/draft-06/schema#" => Some(SpecVersion::Draft06),
            "http://json-schema.org/draft-07/schema#" => Some(SpecVersion::Draft07),
            "https://json-schema.org/draft/2019-09/schema" => Some(SpecVersion::Draft201909),
            "https://json-schema.org/draft/2020-12/schema" => Some(SpecVersion::Draft202012),
            _ => None,
        }
    }

    /// Returns [`JsonSchemaSettings`] tuned for this spec version.
    /// Callers can use the builder to override individual options.
    ///
    /// **Default (latest) spec:** [`Draft202012`](SpecVersion::Draft202012) is
    /// the latest supported spec; its settings match
    /// `JsonSchemaSettings::default()` when no options are set.
    #[must_use]
    pub fn default_schema_settings(self) -> JsonSchemaSettings {
        JsonSchemaSettings {
            disallow_unknown_fields: false,
            spec_version: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::json_schema::SpecVersion;
    use crate::json_schema::settings::JsonSchemaSettings;

    #[test]
    fn spec_version_returns_settings() {
        let settings: JsonSchemaSettings = SpecVersion::Draft07.default_schema_settings();
        assert!(!settings.disallow_unknown_fields);
    }

    /// Default builder output matches Draft 2020-12 (latest spec) settings.
    #[test]
    fn default_settings_match_draft202012() {
        let from_builder: JsonSchemaSettings = JsonSchemaSettings::default();
        let from_spec: JsonSchemaSettings = SpecVersion::Draft202012.default_schema_settings();
        assert_eq!(from_builder, from_spec);
    }

    // --- schema_uri() exhaustive: one expected URI per variant ---

    #[test]
    fn schema_uri_draft00() {
        let expected: &str = "http://json-schema.org/draft-00/hyper-schema#";
        let actual: &str = SpecVersion::Draft00.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft01() {
        let expected: &str = "http://json-schema.org/draft-01/hyper-schema#";
        let actual: &str = SpecVersion::Draft01.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft02() {
        let expected: &str = "http://json-schema.org/draft-02/hyper-schema#";
        let actual: &str = SpecVersion::Draft02.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft03() {
        let expected: &str = "http://json-schema.org/draft-03/schema#";
        let actual: &str = SpecVersion::Draft03.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft04() {
        let expected: &str = "http://json-schema.org/draft-04/schema#";
        let actual: &str = SpecVersion::Draft04.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft05() {
        let expected: &str = "http://json-schema.org/draft-05/schema#";
        let actual: &str = SpecVersion::Draft05.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft06() {
        let expected: &str = "http://json-schema.org/draft-06/schema#";
        let actual: &str = SpecVersion::Draft06.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft07() {
        let expected: &str = "http://json-schema.org/draft-07/schema#";
        let actual: &str = SpecVersion::Draft07.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft201909() {
        let expected: &str = "https://json-schema.org/draft/2019-09/schema";
        let actual: &str = SpecVersion::Draft201909.schema_uri();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uri_draft202012() {
        let expected: &str = "https://json-schema.org/draft/2020-12/schema";
        let actual: &str = SpecVersion::Draft202012.schema_uri();
        assert_eq!(expected, actual);
    }

    // --- from_schema_uri() round-trip: every variant ---

    #[test]
    fn from_schema_uri_round_trip_draft00() {
        let v: SpecVersion = SpecVersion::Draft00;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft01() {
        let v: SpecVersion = SpecVersion::Draft01;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft02() {
        let v: SpecVersion = SpecVersion::Draft02;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft03() {
        let v: SpecVersion = SpecVersion::Draft03;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft04() {
        let v: SpecVersion = SpecVersion::Draft04;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft05() {
        let v: SpecVersion = SpecVersion::Draft05;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft06() {
        let v: SpecVersion = SpecVersion::Draft06;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft07() {
        let v: SpecVersion = SpecVersion::Draft07;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft201909() {
        let v: SpecVersion = SpecVersion::Draft201909;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_round_trip_draft202012() {
        let v: SpecVersion = SpecVersion::Draft202012;
        let expected: Option<SpecVersion> = Some(v);
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri(v.schema_uri());
        assert_eq!(expected, actual);
    }

    // --- from_schema_uri() from string: canonical URIs ---

    #[test]
    fn from_schema_uri_canonical_2020_12() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft202012);
        let actual: Option<SpecVersion> =
            SpecVersion::from_schema_uri("https://json-schema.org/draft/2020-12/schema");
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_canonical_2019_09() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft201909);
        let actual: Option<SpecVersion> =
            SpecVersion::from_schema_uri("https://json-schema.org/draft/2019-09/schema");
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_canonical_draft07() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft07);
        let actual: Option<SpecVersion> =
            SpecVersion::from_schema_uri("http://json-schema.org/draft-07/schema#");
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_canonical_draft04() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft04);
        let actual: Option<SpecVersion> =
            SpecVersion::from_schema_uri("http://json-schema.org/draft-04/schema#");
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_legacy_draft04_schema_hash() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft04);
        let actual: Option<SpecVersion> =
            SpecVersion::from_schema_uri("http://json-schema.org/schema#");
        assert_eq!(expected, actual);
    }

    // --- Unknown / invalid ---

    #[test]
    fn from_schema_uri_empty_returns_none() {
        let expected: Option<SpecVersion> = None;
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_whitespace_only_returns_none() {
        let expected: Option<SpecVersion> = None;
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri("   ");
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_unknown_returns_none() {
        let expected: Option<SpecVersion> = None;
        let actual: Option<SpecVersion> =
            SpecVersion::from_schema_uri("https://unknown.example.com/schema");
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_malformed_returns_none() {
        let expected: Option<SpecVersion> = None;
        let actual: Option<SpecVersion> = SpecVersion::from_schema_uri("not-a-uri");
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_schema_uri_trailing_slash_normalized() {
        // We trim trailing slash; draft/2020-12/schema has no trailing slash in canonical form,
        // so with trailing slash it may not match unless we add that in from_schema_uri.
        // Current implementation does exact match after trim_end_matches('/') only for the string
        // that already has no trailing slash. So "https://json-schema.org/draft/2020-12/schema/"
        // becomes "https://json-schema.org/draft/2020-12/schema" and matches.
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft202012);
        let actual: Option<SpecVersion> =
            SpecVersion::from_schema_uri("https://json-schema.org/draft/2020-12/schema/");
        assert_eq!(expected, actual);
    }
}
