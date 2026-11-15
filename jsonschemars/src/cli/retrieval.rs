//! Retrieval of documents a `$ref` names, shared by every subcommand that reads a schema:
//! `--file-root`, `--allow-remote-refs` and the HTTP client's options.

use clap::{Arg, ArgMatches};
use json_schema_rs::{FileRetriever, Retrieve, RetrieveError};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

/// The retrieval arguments shared by every subcommand that reads a schema.
pub(crate) fn retrieval_args() -> [Arg; 6] {
    [
        Arg::new("file-root")
            .long("file-root")
            .value_name("DIR")
            .help("A $ref to a file that was not given on the command line is read only from inside DIR (default: the current directory)"),
        Arg::new("allow-remote-refs")
            .long("allow-remote-refs")
            .action(clap::ArgAction::SetTrue)
            .help("Retrieve http: and https: documents a $ref names (needs the `http` feature; default: off)"),
        Arg::new("connect-timeout")
            .long("connect-timeout")
            .value_name("SECONDS")
            .value_parser(parse_seconds)
            .requires("allow-remote-refs")
            .help("Give up connecting to a server after SECONDS (fractions allowed; default: no limit)"),
        Arg::new("timeout")
            .long("timeout")
            .value_name("SECONDS")
            .value_parser(parse_seconds)
            .requires("allow-remote-refs")
            .help("Give up on a request after SECONDS in total (fractions allowed; default: 30)"),
        Arg::new("insecure")
            .short('k')
            .long("insecure")
            .action(clap::ArgAction::SetTrue)
            .requires("allow-remote-refs")
            .help("Accept any TLS certificate, even an invalid or self-signed one (default: off)"),
        Arg::new("cacert")
            .long("cacert")
            .value_name("FILE")
            .requires("allow-remote-refs")
            .help("Also trust the certificate authorities in this PEM file"),
    ]
}

/// A duration in seconds: a non-negative, finite number, fractions allowed.
fn parse_seconds(text: &str) -> Result<Duration, String> {
    let seconds: f64 = text
        .parse()
        .map_err(|_| format!("{text:?} is not a number of seconds"))?;
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| format!("{text:?} is not a non-negative, finite number of seconds"))
}

/// Where `$ref`s to documents that were not given on the command line may be read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefRetrieval {
    /// `--file-root`: files are read only from inside it (default: the current directory).
    pub(crate) file_root: Option<PathBuf>,
    /// `--allow-remote-refs`: retrieve `http:` / `https:` documents.
    pub(crate) allow_remote: bool,
    /// How remote documents are requested.
    pub(crate) http: HttpOptions,
}

/// The HTTP client's options: `--connect-timeout`, `--timeout`, `--insecure` and `--cacert`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HttpOptions {
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) insecure: bool,
    pub(crate) cacert: Option<PathBuf>,
}

impl RefRetrieval {
    /// The retrieval settings from the arguments added by [`retrieval_args`].
    pub(crate) fn from_matches(matches: &ArgMatches) -> Self {
        RefRetrieval {
            file_root: matches.get_one::<String>("file-root").map(PathBuf::from),
            allow_remote: matches.get_flag("allow-remote-refs"),
            http: HttpOptions {
                connect_timeout: matches.get_one::<Duration>("connect-timeout").copied(),
                timeout: matches.get_one::<Duration>("timeout").copied(),
                insecure: matches.get_flag("insecure"),
                cacert: matches.get_one::<String>("cacert").map(PathBuf::from),
            },
        }
    }
}

/// The HTTP retriever `options` configure.
#[cfg(feature = "http")]
fn http_retriever(options: &HttpOptions) -> Result<json_schema_rs::HttpRetriever, String> {
    let mut builder: reqwest::blocking::ClientBuilder = reqwest::blocking::Client::builder();
    if let Some(timeout) = options.connect_timeout {
        builder = builder.connect_timeout(timeout);
    }
    if let Some(timeout) = options.timeout {
        builder = builder.timeout(timeout);
    }
    if options.insecure {
        builder = builder.tls_danger_accept_invalid_certs(true);
    }
    if let Some(path) = &options.cacert {
        let pem: Vec<u8> = std::fs::read(path)
            .map_err(|e| format!("failed to read --cacert {}: {e}", path.display()))?;
        let certificates: Vec<reqwest::Certificate> = reqwest::Certificate::from_pem_bundle(&pem)
            .map_err(|e| {
            format!(
                "--cacert {} is not a PEM certificate bundle: {e}",
                path.display()
            )
        })?;
        if certificates.is_empty() {
            return Err(format!("--cacert {} holds no certificate", path.display()));
        }
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
    }
    let client: reqwest::blocking::Client = builder
        .build()
        .map_err(|e| format!("the HTTP client cannot be created: {e}"))?;
    Ok(json_schema_rs::HttpRetriever::with_client(client))
}

/// The CLI's retriever: `file:` URIs inside the file root, and `http:` / `https:` URLs when
/// remote retrieval is enabled.
#[derive(Clone)]
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
            Some(http_retriever(&retrieval.http)?)
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
            .map_or_default(|(scheme, _)| scheme.to_ascii_lowercase());
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
