//! Retrieval of documents a `$ref` names, shared by every subcommand that reads a schema:
//! `--file-root` and `--allow-remote-refs`.

use clap::{Arg, ArgMatches};
use json_schema_rs::{FileRetriever, Retrieve, RetrieveError};
use serde_json::Value;
use std::path::PathBuf;

/// The retrieval arguments shared by `generate` and `validate`.
pub(crate) fn retrieval_args() -> [Arg; 2] {
    [
        Arg::new("file-root")
            .long("file-root")
            .value_name("DIR")
            .help("A $ref to a file that was not given on the command line is read only from inside DIR (default: the current directory)"),
        Arg::new("allow-remote-refs")
            .long("allow-remote-refs")
            .action(clap::ArgAction::SetTrue)
            .help("Retrieve http: and https: documents a $ref names (needs the `http` feature; default: off)"),
    ]
}

/// Where `$ref`s to documents that were not given on the command line may be read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefRetrieval {
    /// `--file-root`: files are read only from inside it (default: the current directory).
    pub(crate) file_root: Option<PathBuf>,
    /// `--allow-remote-refs`: retrieve `http:` / `https:` documents.
    pub(crate) allow_remote: bool,
}

impl RefRetrieval {
    /// The retrieval settings from the arguments added by [`retrieval_args`].
    pub(crate) fn from_matches(matches: &ArgMatches) -> Self {
        RefRetrieval {
            file_root: matches.get_one::<String>("file-root").map(PathBuf::from),
            allow_remote: matches.get_flag("allow-remote-refs"),
        }
    }
}

/// The CLI's retriever: `file:` URIs inside the file root, and `http:` / `https:` URLs when
/// remote retrieval is enabled.
pub(crate) struct CliRetriever {
    file: FileRetriever,
    #[cfg(feature = "http")]
    http: Option<json_schema_rs::HttpRetriever>,
}

impl CliRetriever {
    pub(crate) fn new(retrieval: &RefRetrieval) -> Result<Self, String> {
        let file_root: PathBuf = match &retrieval.file_root {
            Some(root) => root.clone(),
            None => std::env::current_dir()
                .map_err(|e| format!("cannot read the current directory: {e}"))?,
        };
        #[cfg(feature = "http")]
        let http: Option<json_schema_rs::HttpRetriever> = if retrieval.allow_remote {
            Some(json_schema_rs::HttpRetriever::new().map_err(|e| e.reason)?)
        } else {
            None
        };
        #[cfg(not(feature = "http"))]
        if retrieval.allow_remote {
            return Err(
                "--allow-remote-refs needs jsonschemars built with the `http` feature".to_string(),
            );
        }
        Ok(CliRetriever {
            file: FileRetriever::new(file_root),
            #[cfg(feature = "http")]
            http,
        })
    }
}

impl Retrieve for CliRetriever {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        let scheme: String = uri
            .split_once(':')
            .map(|(scheme, _)| scheme.to_ascii_lowercase())
            .unwrap_or_default();
        match scheme.as_str() {
            "file" => self.file.retrieve(uri).map_err(|e| {
                // Point at the flag only when the root is what refused the file.
                if e.reason.contains(" is outside the file root ") {
                    RetrieveError {
                        reason: format!("{} (set it with --file-root)", e.reason),
                    }
                } else {
                    e
                }
            }),
            "http" | "https" => {
                #[cfg(feature = "http")]
                if let Some(http) = &self.http {
                    return http.retrieve(uri);
                }
                Err(RetrieveError {
                    reason: "remote retrieval is off (enable it with --allow-remote-refs)"
                        .to_string(),
                })
            }
            _ => Err(RetrieveError {
                reason: format!("no retriever for the URI scheme \"{scheme}\""),
            }),
        }
    }
}
