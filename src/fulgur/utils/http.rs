//! HTTP client used by the GPUI image loader for Markdown preview images.

use futures::future::BoxFuture;
use gpui_kit::http_client::{AsyncBody, HttpClient, Request, Response, StatusCode, Url};
use reqwest_client::ReqwestClient;
use std::fs::File;
use std::io::Read as _;
use std::path::{Component, Path, Prefix};

/// An [`HttpClient`] that serves `file://` URLs from the local filesystem and
/// delegates every other request to an inner [`ReqwestClient`].
pub struct FileAwareHttpClient {
    inner: ReqwestClient,
}

impl FileAwareHttpClient {
    /// Build a client with a Fulgur user agent.
    ///
    /// ### Errors
    /// Returns an error if the inner [`ReqwestClient`] cannot be constructed
    /// (for example when the user agent header is invalid).
    ///
    /// ### Returns
    /// - `Ok(FileAwareHttpClient)`: A ready-to-use client.
    /// - `Err(anyhow::Error)`: The inner client could not be created.
    pub fn new() -> anyhow::Result<Self> {
        let user_agent = concat!("Fulgur/", env!("CARGO_PKG_VERSION"));
        let inner = ReqwestClient::user_agent(user_agent)?;
        Ok(Self { inner })
    }
}

/// Largest local image the Markdown preview will load into memory.
const MAX_LOCAL_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Return whether `path` targets a network share or uses a verbatim prefix.
///
/// On Windows, opening a UNC path (`\\server\share`) connects to the remote
/// host over SMB and leaks the user's NTLM credentials, and verbatim prefixes
/// (`\\?\`, `\\.\`) bypass normal path handling. Only plain drive-letter
/// paths are treated as local. Paths never carry a prefix on other platforms.
///
/// ### Arguments
/// - `path`: The filesystem path to classify.
///
/// ### Returns
/// - `true`: The path starts with a UNC, verbatim, or device prefix.
/// - `false`: The path is a plain local path.
pub(crate) fn is_network_or_verbatim_path(path: &Path) -> bool {
    matches!(
        path.components().next(),
        Some(Component::Prefix(prefix)) if !matches!(prefix.kind(), Prefix::Disk(_))
    )
}

/// Build a synthetic HTTP response carrying a local file read from `uri`.
///
/// Only regular files on the local machine up to [`MAX_LOCAL_IMAGE_BYTES`] are
/// served, so an untrusted document cannot make the preview read a device
/// (`/dev/zero`), block on a FIFO, load a huge file, or reach a network share.
///
/// ### Arguments
/// - `uri`: A `file://` URI pointing at a local file.
///
/// ### Returns
/// - `Ok(Response<AsyncBody>)`: A `200 OK` response whose body is the file bytes.
/// - `Err(anyhow::Error)`: The URI could not be parsed, names a remote host, a
///   network path, a non-regular or oversized file, or the file could not be read.
fn read_file_uri(uri: &str) -> anyhow::Result<Response<AsyncBody>> {
    let url = Url::parse(uri).map_err(|e| anyhow::anyhow!("invalid file URL {uri}: {e}"))?;
    if let Some(host) = url.host_str()
        && !host.is_empty()
        && !host.eq_ignore_ascii_case("localhost")
    {
        anyhow::bail!("file URL names a remote host: {uri}");
    }
    let path = url
        .to_file_path()
        .map_err(|()| anyhow::anyhow!("file URL is not a local path: {uri}"))?;
    if is_network_or_verbatim_path(&path) {
        anyhow::bail!("file URL is not a plain local path: {uri}");
    }

    let metadata = std::fs::metadata(&path)
        .map_err(|e| anyhow::anyhow!("reading metadata of {}: {e}", path.display()))?;
    if !metadata.is_file() {
        anyhow::bail!("not a regular file: {}", path.display());
    }
    if metadata.len() > MAX_LOCAL_IMAGE_BYTES {
        anyhow::bail!(
            "{} is {} bytes, above the {MAX_LOCAL_IMAGE_BYTES} bytes image limit",
            path.display(),
            metadata.len()
        );
    }

    let file = File::open(&path).map_err(|e| anyhow::anyhow!("opening {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_LOCAL_IMAGE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_LOCAL_IMAGE_BYTES {
        anyhow::bail!(
            "{} grew above the {MAX_LOCAL_IMAGE_BYTES} bytes image limit while reading",
            path.display()
        );
    }

    Response::builder()
        .status(StatusCode::OK)
        .body(AsyncBody::from(bytes))
        .map_err(|e| anyhow::anyhow!("building file response for {uri}: {e}"))
}

impl HttpClient for FileAwareHttpClient {
    fn user_agent(&self) -> Option<&gpui_kit::http_client::http::HeaderValue> {
        self.inner.user_agent()
    }

    fn proxy(&self) -> Option<&Url> {
        self.inner.proxy()
    }

    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        // `file://` is served through `get`, which the GPUI image loader calls;
        // `http::Uri` cannot represent a `file://` target, so it never reaches
        // `send`. Every real `send` request is therefore remote.
        self.inner.send(req)
    }

    fn get(
        &self,
        uri: &str,
        body: AsyncBody,
        follow_redirects: bool,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        if uri.starts_with("file://") {
            let uri = uri.to_string();
            return Box::pin(async move { read_file_uri(&uri) });
        }
        self.inner.get(uri, body, follow_redirects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::AsyncReadExt as _;
    use std::io::Write as _;

    fn read_file_uri_error(uri: &str) -> String {
        match read_file_uri(uri) {
            Ok(_) => panic!("expected {uri} to be rejected"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn read_file_uri_serves_local_bytes() {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(b"local-image-bytes").expect("write");
        let url = Url::from_file_path(file.path()).expect("file url");

        let response = read_file_uri(url.as_str()).expect("response");
        assert_eq!(response.status(), StatusCode::OK);

        let mut body = Vec::new();
        futures::executor::block_on(response.into_body().read_to_end(&mut body))
            .expect("read body");
        assert_eq!(body, b"local-image-bytes");
    }

    #[test]
    fn read_file_uri_rejects_missing_file() {
        let missing = std::env::temp_dir().join("fulgur-nonexistent-image.png");
        let _ = std::fs::remove_file(&missing);
        let url = Url::from_file_path(&missing).expect("file url");
        assert!(read_file_uri(url.as_str()).is_err());
    }

    #[test]
    fn read_file_uri_rejects_remote_host() {
        let error = read_file_uri_error("file://attacker/share/x.png");
        assert!(error.contains("remote host"), "{error}");
    }

    #[test]
    fn read_file_uri_accepts_localhost_host() {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(b"bytes").expect("write");
        let mut url = Url::from_file_path(file.path()).expect("file url");
        url.set_host(Some("localhost")).expect("set host");
        assert!(read_file_uri(url.as_str()).is_ok());
    }

    #[test]
    fn read_file_uri_rejects_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        let url = Url::from_directory_path(dir.path()).expect("dir url");
        let error = read_file_uri_error(url.as_str());
        assert!(error.contains("not a regular file"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn read_file_uri_rejects_dev_zero() {
        let error = read_file_uri_error("file:///dev/zero");
        assert!(error.contains("not a regular file"), "{error}");
    }

    #[test]
    fn read_file_uri_rejects_oversized_file() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        file.as_file()
            .set_len(MAX_LOCAL_IMAGE_BYTES + 1)
            .expect("grow sparse file");
        let url = Url::from_file_path(file.path()).expect("file url");
        let error = read_file_uri_error(url.as_str());
        assert!(error.contains("image limit"), "{error}");
    }

    #[test]
    fn plain_local_paths_are_not_network_paths() {
        let local = std::env::temp_dir().join("image.png");
        assert!(!is_network_or_verbatim_path(&local));
    }

    #[cfg(windows)]
    #[test]
    fn unc_and_verbatim_paths_are_network_paths() {
        assert!(is_network_or_verbatim_path(Path::new(
            r"\\attacker\share\x.png"
        )));
        assert!(is_network_or_verbatim_path(Path::new(r"\\?\C:\x.png")));
        assert!(is_network_or_verbatim_path(Path::new(r"\\.\pipe\x")));
        assert!(!is_network_or_verbatim_path(Path::new(r"C:\x.png")));
    }
}
