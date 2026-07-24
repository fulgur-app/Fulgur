//! HTTP client used by the GPUI image loader for Markdown preview images.

use futures::future::BoxFuture;
use http_client::{AsyncBody, HttpClient, Request, Response, StatusCode, Url};
use reqwest_client::ReqwestClient;

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

/// Build a synthetic HTTP response carrying a local file read from `uri`.
///
/// ### Arguments
/// - `uri`: A `file://` URI pointing at a local file.
///
/// ### Returns
/// - `Ok(Response<AsyncBody>)`: A `200 OK` response whose body is the file bytes.
/// - `Err(anyhow::Error)`: The URI could not be parsed, mapped to a path, or read.
fn read_file_uri(uri: &str) -> anyhow::Result<Response<AsyncBody>> {
    let url = Url::parse(uri).map_err(|e| anyhow::anyhow!("invalid file URL {uri}: {e}"))?;
    let path = url
        .to_file_path()
        .map_err(|()| anyhow::anyhow!("file URL is not a local path: {uri}"))?;
    let bytes =
        std::fs::read(&path).map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    Response::builder()
        .status(StatusCode::OK)
        .body(AsyncBody::from(bytes))
        .map_err(|e| anyhow::anyhow!("building file response for {uri}: {e}"))
}

impl HttpClient for FileAwareHttpClient {
    fn user_agent(&self) -> Option<&http_client::http::HeaderValue> {
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
}
