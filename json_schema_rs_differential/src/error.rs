//! Error type for the differential report builder.

use std::fmt;
use std::path::PathBuf;

/// Everything that can go wrong while loading inputs or building the differential report.
#[derive(Debug)]
pub enum DifferentialError {
    /// Reading or writing a file or directory failed.
    Io {
        /// The file or directory involved.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// A JSON input (suite file, manifest, fixture) could not be parsed.
    Json {
        /// The file involved.
        path: PathBuf,
        /// The underlying serde error.
        source: serde_json::Error,
    },
    /// A Bowtie report line was malformed or the report was structurally inconsistent.
    Report {
        /// The report file involved.
        path: PathBuf,
        /// The parse error (carries the 1-based line number).
        source: crate::report::ReportError,
    },
    /// A report file the driver was supposed to produce does not exist.
    MissingReport {
        /// The expected report path.
        path: PathBuf,
    },
    /// The implementation passed via `--ours` does not appear in a report.
    MissingImplementation {
        /// The `--ours` implementation id.
        ours: String,
        /// The report file that lacks it.
        path: PathBuf,
        /// The implementation ids the report does contain.
        available: Vec<String>,
    },
    /// The benchmark fixture manifest is inconsistent with a Bowtie fixture report.
    Fixture {
        /// The fixture id.
        id: String,
        /// What is inconsistent.
        message: String,
    },
}

impl fmt::Display for DifferentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DifferentialError::Io { path, source } => {
                write!(f, "I/O error on {}: {source}", path.display())
            }
            DifferentialError::Json { path, source } => {
                write!(f, "invalid JSON in {}: {source}", path.display())
            }
            DifferentialError::Report { path, source } => {
                write!(f, "invalid Bowtie report {}: {source}", path.display())
            }
            DifferentialError::MissingReport { path } => write!(
                f,
                "missing Bowtie report {} (the driver must produce every draft and fixture report)",
                path.display()
            ),
            DifferentialError::MissingImplementation {
                ours,
                path,
                available,
            } => write!(
                f,
                "implementation \"{ours}\" does not appear in {} (available: {})",
                path.display(),
                available.join(", ")
            ),
            DifferentialError::Fixture { id, message } => {
                write!(f, "fixture \"{id}\": {message}")
            }
        }
    }
}

impl std::error::Error for DifferentialError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DifferentialError::Io { source, .. } => Some(source),
            DifferentialError::Json { source, .. } => Some(source),
            DifferentialError::Report { source, .. } => Some(source),
            DifferentialError::MissingReport { .. }
            | DifferentialError::MissingImplementation { .. }
            | DifferentialError::Fixture { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DifferentialError;
    use std::path::PathBuf;

    #[test]
    fn display_missing_report() {
        let error: DifferentialError = DifferentialError::MissingReport {
            path: PathBuf::from("reports/suite/draft7.jsonl"),
        };
        let expected: &str = "missing Bowtie report reports/suite/draft7.jsonl (the driver must produce every draft and fixture report)";
        let actual: String = error.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn display_missing_implementation() {
        let error: DifferentialError = DifferentialError::MissingImplementation {
            ours: "localhost/json-schema-rs".to_string(),
            path: PathBuf::from("r.jsonl"),
            available: vec!["rust-boon".to_string(), "rust-jsonschema".to_string()],
        };
        let expected: &str = "implementation \"localhost/json-schema-rs\" does not appear in r.jsonl (available: rust-boon, rust-jsonschema)";
        let actual: String = error.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn display_fixture() {
        let error: DifferentialError = DifferentialError::Fixture {
            id: "small/config".to_string(),
            message: "expected 2 tests, report has 3".to_string(),
        };
        let expected: &str = "fixture \"small/config\": expected 2 tests, report has 3";
        let actual: String = error.to_string();
        assert_eq!(expected, actual);
    }
}
