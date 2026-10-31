//! Built-in [`Retrieve`] implementations: local files under a root directory ([`FileRetriever`])
//! and, with the `http` feature, `http:` / `https:` URLs ([`HttpRetriever`]).
//!
//! Neither is used unless a caller passes it to a registry or to codegen: the default retriever
//! is [`OfflineRetriever`](super::OfflineRetriever). The CLI and the `json_schema_to_rust!` macro
//! use a [`FileRetriever`] (see design.md, *Schema registry*).

use super::registry::{Retrieve, RetrieveError};
use fluent_uri::Iri;
use fluent_uri::pct_enc::EString;
use fluent_uri::pct_enc::encoder::IPath;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The `file:` URI of an absolute `path` (`/a/b c.json` → `file:///a/b%20c.json`), or `None`
/// when `path` is relative or not UTF-8.
#[must_use]
pub fn file_uri(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    let text: &str = path.to_str()?;
    // Windows: `C:\a\b` → `/C:/a/b`.
    let text: String = if cfg!(windows) {
        format!("/{}", text.replace('\\', "/"))
    } else {
        text.to_string()
    };
    let mut encoded: EString<IPath> = EString::new();
    encoded.encode_str::<IPath>(&text);
    Some(format!("file://{}", encoded.as_str()))
}

/// The local path a `file:` URI names (the inverse of [`file_uri`]).
fn file_path(uri: &str) -> Result<PathBuf, String> {
    let parsed: Iri<&str> = Iri::parse(uri).map_err(|e| e.to_string())?;
    if !parsed.scheme().as_str().eq_ignore_ascii_case("file") {
        return Err("it is not a file: URI".to_string());
    }
    if let Some(authority) = parsed.authority()
        && !authority.as_str().is_empty()
        && !authority.as_str().eq_ignore_ascii_case("localhost")
    {
        return Err(format!(
            "it names the host \"{}\"; only local files are read",
            authority.as_str()
        ));
    }
    let path: String = parsed
        .path()
        .decode()
        .to_string()
        .map_err(|_| "its decoded path is not UTF-8".to_string())?
        .into_owned();
    // Windows: `/C:/a/b` → `C:/a/b`.
    let path: &str = if cfg!(windows) {
        path.strip_prefix('/').unwrap_or(&path)
    } else {
        &path
    };
    Ok(PathBuf::from(path))
}

/// Reads `file:` URIs from the local file system, only inside a root directory.
///
/// The URI's path is resolved through symbolic links and `..` before the check, so nothing
/// outside the root is read. Any other scheme, a file outside the root, a file that cannot be
/// read and a file that is not JSON are each a [`RetrieveError`] naming the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRetriever {
    root: PathBuf,
}

impl FileRetriever {
    /// A retriever for files under `root` (a directory, at any depth).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FileRetriever { root: root.into() }
    }

    /// The directory files must be under.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Retrieve for FileRetriever {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        let error = |reason: String| RetrieveError { reason };
        let path: PathBuf = file_path(uri).map_err(error)?;
        let root: PathBuf = self.root.canonicalize().map_err(|e| {
            error(format!(
                "the file root {} cannot be read: {e}",
                self.root.display()
            ))
        })?;
        let path: PathBuf = path
            .canonicalize()
            .map_err(|e| error(format!("{} cannot be read: {e}", path.display())))?;
        if !path.starts_with(&root) {
            return Err(error(format!(
                "{} is outside the file root {}",
                path.display(),
                root.display()
            )));
        }
        let text: String = std::fs::read_to_string(&path)
            .map_err(|e| error(format!("{} cannot be read: {e}", path.display())))?;
        serde_json::from_str(&text)
            .map_err(|e| error(format!("{} is not JSON: {e}", path.display())))
    }
}

/// Fetches `http:` and `https:` URLs with a blocking [`reqwest`] client (its default timeout,
/// redirect policy, proxy and TLS settings). Any other scheme, a request that fails, a status
/// that is not a success and a body that is not JSON are each a [`RetrieveError`].
///
/// Only with the `http` feature. Retrieving a URL a schema names lets that schema make network
/// requests from the machine running the generator or validator, so enable it only for schemas
/// you trust.
#[cfg(feature = "http")]
#[derive(Debug, Clone)]
pub struct HttpRetriever {
    client: reqwest::blocking::Client,
}

#[cfg(feature = "http")]
impl HttpRetriever {
    /// A retriever with a default client.
    ///
    /// # Errors
    ///
    /// Returns [`RetrieveError`] when the HTTP client cannot be created (no TLS backend).
    pub fn new() -> Result<Self, RetrieveError> {
        let client: reqwest::blocking::Client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|e| RetrieveError {
                reason: format!("the HTTP client cannot be created: {e}"),
            })?;
        Ok(HttpRetriever { client })
    }

    /// A retriever with a caller-configured client (timeouts, proxies, headers).
    #[must_use]
    pub fn with_client(client: reqwest::blocking::Client) -> Self {
        HttpRetriever { client }
    }
}

#[cfg(feature = "http")]
impl Retrieve for HttpRetriever {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        let error = |reason: String| RetrieveError { reason };
        let scheme: String = Iri::parse(uri)
            .map_err(|e| error(e.to_string()))?
            .scheme()
            .as_str()
            .to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(error("it is not an http: or https: URL".to_string()));
        }
        let response: reqwest::blocking::Response = self
            .client
            .get(uri)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|e| error(format!("the request failed: {e}")))?;
        let text: String = response
            .text()
            .map_err(|e| error(format!("the response body cannot be read: {e}")))?;
        serde_json::from_str(&text).map_err(|e| error(format!("the response is not JSON: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::{FileRetriever, file_path, file_uri};
    use crate::json_schema::registry::{Retrieve, RetrieveError};
    use serde_json::{Value, json};
    use std::path::{Path, PathBuf};

    #[test]
    #[cfg(not(windows))]
    fn file_uri_percent_encodes_space_and_percent() {
        let expected: Option<String> = Some("file:///a/b%20c%25d.json".to_string());
        let actual: Option<String> = file_uri(Path::new("/a/b c%d.json"));
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(windows))]
    fn file_uri_keeps_non_ascii_characters() {
        let expected: Option<String> = Some("file:///a/é.json".to_string());
        let actual: Option<String> = file_uri(Path::new("/a/é.json"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn file_uri_of_relative_path_is_none() {
        let expected: Option<String> = None;
        let actual: Option<String> = file_uri(Path::new("a/b.json"));
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(windows))]
    fn file_path_decodes_percent_escapes() {
        let expected: Result<PathBuf, String> = Ok(PathBuf::from("/a/b c.json"));
        let actual: Result<PathBuf, String> = file_path("file:///a/b%20c.json");
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(windows))]
    fn file_path_accepts_localhost_authority() {
        let expected: Result<PathBuf, String> = Ok(PathBuf::from("/a.json"));
        let actual: Result<PathBuf, String> = file_path("file://localhost/a.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn file_path_rejects_remote_host() {
        let expected: Result<PathBuf, String> =
            Err("it names the host \"example.com\"; only local files are read".to_string());
        let actual: Result<PathBuf, String> = file_path("file://example.com/a.json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn file_path_rejects_other_scheme() {
        let expected: Result<PathBuf, String> = Err("it is not a file: URI".to_string());
        let actual: Result<PathBuf, String> = file_path("https://example.com/a.json");
        assert_eq!(expected, actual);
    }

    /// A temporary root with `inside/a.json` (an object), `inside/bad.json` (not JSON) and
    /// `outside.json` next to the root.
    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir: tempfile::TempDir = tempfile::tempdir().expect("tempdir");
        let root: PathBuf = dir.path().join("inside");
        std::fs::create_dir(&root).expect("mkdir");
        std::fs::write(root.join("a.json"), r#"{"type":"string"}"#).expect("write");
        std::fs::write(root.join("bad.json"), "{").expect("write");
        std::fs::write(dir.path().join("outside.json"), "{}").expect("write");
        (dir, root)
    }

    fn uri_of(path: &Path) -> String {
        file_uri(&path.canonicalize().expect("canonicalize")).expect("file uri")
    }

    #[test]
    fn file_retriever_reads_file_under_root() {
        let (_dir, root) = tree();
        let retriever: FileRetriever = FileRetriever::new(&root);
        let expected: Result<Value, RetrieveError> = Ok(json!({"type": "string"}));
        let actual: Result<Value, RetrieveError> =
            retriever.retrieve(&uri_of(&root.join("a.json")));
        assert_eq!(expected, actual);
    }

    #[test]
    fn file_retriever_refuses_file_outside_root() {
        let (dir, root) = tree();
        let outside: PathBuf = dir
            .path()
            .join("outside.json")
            .canonicalize()
            .expect("path");
        let canonical_root: PathBuf = root.canonicalize().expect("root");
        let retriever: FileRetriever = FileRetriever::new(&root);
        let expected: Result<Value, RetrieveError> = Err(RetrieveError {
            reason: format!(
                "{} is outside the file root {}",
                outside.display(),
                canonical_root.display()
            ),
        });
        let actual: Result<Value, RetrieveError> = retriever.retrieve(&uri_of(&outside));
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(windows))]
    fn file_retriever_refuses_dot_dot_escape_from_root() {
        let (dir, root) = tree();
        let outside: PathBuf = dir
            .path()
            .join("outside.json")
            .canonicalize()
            .expect("path");
        let canonical_root: PathBuf = root.canonicalize().expect("root");
        let escaping: String = format!(
            "{}/../outside.json",
            file_uri(&canonical_root).expect("file uri")
        );
        let retriever: FileRetriever = FileRetriever::new(&root);
        let expected: Result<Value, RetrieveError> = Err(RetrieveError {
            reason: format!(
                "{} is outside the file root {}",
                outside.display(),
                canonical_root.display()
            ),
        });
        let actual: Result<Value, RetrieveError> = retriever.retrieve(&escaping);
        assert_eq!(expected, actual);
    }

    #[test]
    fn file_retriever_reports_file_that_is_not_json() {
        let (_dir, root) = tree();
        let path: PathBuf = root.join("bad.json").canonicalize().expect("path");
        let retriever: FileRetriever = FileRetriever::new(&root);
        let expected: Result<Value, RetrieveError> = Err(RetrieveError {
            reason: format!(
                "{} is not JSON: EOF while parsing an object at line 1 column 1",
                path.display()
            ),
        });
        let actual: Result<Value, RetrieveError> = retriever.retrieve(&uri_of(&path));
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(windows))]
    fn file_retriever_reports_missing_file() {
        let (_dir, root) = tree();
        let missing: PathBuf = root.canonicalize().expect("root").join("missing.json");
        let retriever: FileRetriever = FileRetriever::new(&root);
        let expected: Result<Value, RetrieveError> = Err(RetrieveError {
            reason: format!(
                "{} cannot be read: No such file or directory (os error 2)",
                missing.display()
            ),
        });
        let actual: Result<Value, RetrieveError> =
            retriever.retrieve(&file_uri(&missing).expect("file uri"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn file_retriever_refuses_other_scheme() {
        let (_dir, root) = tree();
        let retriever: FileRetriever = FileRetriever::new(&root);
        let expected: Result<Value, RetrieveError> = Err(RetrieveError {
            reason: "it is not a file: URI".to_string(),
        });
        let actual: Result<Value, RetrieveError> = retriever.retrieve("https://example.com/a.json");
        assert_eq!(expected, actual);
    }

    /// Serves one HTTP response with `status` and `body` on a local port; returns its URL.
    #[cfg(feature = "http")]
    fn serve_once(status: &'static str, body: &'static str) -> String {
        use std::io::{Read, Write};
        let listener: std::net::TcpListener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address: std::net::SocketAddr = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request: [u8; 1024] = [0; 1024];
            let _ = stream.read(&mut request);
            let response: String = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write");
        });
        format!("http://{address}/schema.json")
    }

    #[test]
    #[cfg(feature = "http")]
    fn http_retriever_fetches_json_document() {
        let url: String = serve_once("200 OK", r#"{"type":"string"}"#);
        let retriever: super::HttpRetriever = super::HttpRetriever::new().expect("client");
        let expected: Result<Value, RetrieveError> = Ok(json!({"type": "string"}));
        let actual: Result<Value, RetrieveError> = retriever.retrieve(&url);
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(feature = "http")]
    fn http_retriever_reports_error_status() {
        let url: String = serve_once("404 Not Found", "");
        let retriever: super::HttpRetriever = super::HttpRetriever::new().expect("client");
        let expected: Result<Value, RetrieveError> = Err(RetrieveError {
            reason: format!(
                "the request failed: HTTP status client error (404 Not Found) for url ({url})"
            ),
        });
        let actual: Result<Value, RetrieveError> = retriever.retrieve(&url);
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(feature = "http")]
    fn http_retriever_refuses_other_scheme() {
        let retriever: super::HttpRetriever = super::HttpRetriever::new().expect("client");
        let expected: Result<Value, RetrieveError> = Err(RetrieveError {
            reason: "it is not an http: or https: URL".to_string(),
        });
        let actual: Result<Value, RetrieveError> = retriever.retrieve("file:///a.json");
        assert_eq!(expected, actual);
    }
}
