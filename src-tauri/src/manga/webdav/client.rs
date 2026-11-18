//! The HTTP side of a WebDAV source: listings, whole small files, byte ranges
//! of big ones and downloads, with timeouts and a few retries.
//!
//! Credentials live in [`Auth`], which only Rust ever holds: it is read from
//! `manga_sources.auth_json`, put on each request by reqwest as a sensitive
//! header (dropped on a redirect to another host) and never formatted, logged
//! or sent to the UI.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use once_cell::sync::Lazy;
use reqwest::{header, Method, RequestBuilder, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};

use super::propfind::{self, Entry, MAX_LISTING_BYTES};

/// How the server wants to be asked. The UI sends one in a source form; Rust
/// never sends one back.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "WebdavAuth"))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Auth {
    #[default]
    None,
    Basic { username: String, password: String },
    Bearer { token: String },
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the secret, not even in a debug log.
        write!(f, "Auth::{}", self.kind())
    }
}

impl Auth {
    /// `none`, `basic` or `bearer`: what the UI may know about it.
    pub fn kind(&self) -> &'static str {
        match self {
            Auth::None => "none",
            Auth::Basic { .. } => "basic",
            Auth::Bearer { .. } => "bearer",
        }
    }

    /// From `manga_sources.auth_json` (absent or unreadable = anonymous).
    pub fn from_json(json: Option<&str>) -> Auth {
        json.and_then(|raw| serde_json::from_str(raw).ok()).unwrap_or_default()
    }

    pub fn to_json(&self) -> Option<String> {
        match self {
            Auth::None => None,
            other => serde_json::to_string(other).ok(),
        }
    }

    fn apply(&self, request: RequestBuilder) -> RequestBuilder {
        match self {
            Auth::None => request,
            Auth::Basic { username, password } => request.basic_auth(username, Some(password)),
            Auth::Bearer { token } => request.bearer_auth(token),
        }
    }
}

/// Why a request failed, as the UI tells it (see [`DavError::code`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DavError {
    #[error("the server could not be reached")]
    Offline,
    #[error("the server refused the credentials")]
    Unauthorized,
    #[error("no such file or folder on the server")]
    NotFound,
    #[error("the address does not answer like a WebDAV server")]
    NotWebdav,
    #[error("the server does not send parts of files")]
    NoRange,
    #[error("the file is larger than expected")]
    TooLarge,
    #[error("the file changed on the server")]
    Changed,
    #[error("the download was cancelled")]
    Cancelled,
    #[error("not a valid http(s) address")]
    InvalidUrl,
    #[error("the server answered {0}")]
    Status(u16),
    #[error("{0}")]
    Other(String),
}

impl DavError {
    /// A stable code for the UI to translate.
    pub fn code(&self) -> &'static str {
        match self {
            DavError::Offline => "offline",
            DavError::Unauthorized => "unauthorized",
            DavError::NotFound => "not-found",
            DavError::NotWebdav => "not-webdav",
            DavError::NoRange => "needs-download",
            DavError::TooLarge => "too-large",
            DavError::Changed => "changed",
            DavError::Cancelled => "cancelled",
            DavError::InvalidUrl => "invalid-url",
            DavError::Status(_) | DavError::Other(_) => "error",
        }
    }

    /// The network, not the server: worth another try, and what "offline" means.
    fn from_reqwest(error: &reqwest::Error) -> DavError {
        if error.is_connect() || error.is_timeout() || error.is_request() || error.is_body() {
            DavError::Offline
        } else {
            DavError::Other(error.without_url_string())
        }
    }
}

trait WithoutUrl {
    fn without_url_string(&self) -> String;
}

impl WithoutUrl for reqwest::Error {
    /// The error without its URL: a URL is only a path here, but a log line
    /// does not need it.
    fn without_url_string(&self) -> String {
        let mut text = self.to_string();
        if let Some(url) = self.url() {
            text = text.replace(url.as_str(), "<url>");
        }
        text
    }
}

const ATTEMPTS: u32 = 3;
const LISTING_TIMEOUT: Duration = Duration::from_secs(60);
const FILE_TIMEOUT: Duration = Duration::from_secs(90);

/// One pool for every WebDAV request. No overall timeout on the client (a
/// 600 MB download takes as long as it takes); a stall is caught by
/// `read_timeout`, and each kind of request sets its own deadline.
static HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .user_agent(concat!("NihongoTrackerCompanion/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest client")
});

/// Reads what the user typed as a server address: `http(s)` only, without a
/// fragment or query, ending in `/`. A `user:password@` in it is taken out of
/// the address (it would end up in the database and in logs) and returned as
/// Basic credentials.
pub fn parse_base_url(input: &str) -> Result<(Url, Option<Auth>), DavError> {
    let mut url = Url::parse(input.trim()).map_err(|_| DavError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(DavError::InvalidUrl);
    }
    let auth = (!url.username().is_empty()).then(|| Auth::Basic {
        username: propfind::decode_path(url.username()),
        password: propfind::decode_path(url.password().unwrap_or("")),
    });
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok((url, auth))
}

/// Characters left as they are in a path segment: RFC 3986 unreserved.
const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// A decoded absolute path, encoded segment by segment for a URL.
pub fn encode_path(path: &str) -> String {
    path.split('/').map(|segment| percent_encoding::utf8_percent_encode(segment, SEGMENT).to_string()).collect::<Vec<_>>().join("/")
}

/// A WebDAV server, as one source sees it.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
    auth: Auth,
}

impl Client {
    pub fn new(base: Url, auth: Auth) -> Self {
        Client { http: HTTP.clone(), base, auth }
    }

    /// The decoded path of the address the user gave (`/dav/`).
    pub fn base_path(&self) -> String {
        propfind::normalize(&propfind::decode_path(self.base.path())).unwrap_or_else(|| "/".into())
    }

    /// The URL of a decoded absolute path on this server. Only the path
    /// changes: a listing can never send us to another host.
    pub fn url_for(&self, path: &str) -> Url {
        let mut url = self.base.clone();
        url.set_path(&encode_path(path));
        url
    }

    /// Sends a request built by `build`, again on a network failure or a busy
    /// server, with a short backoff.
    async fn send(&self, build: impl Fn() -> RequestBuilder) -> Result<Response, DavError> {
        let mut last = DavError::Offline;
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(400 * 3u64.pow(attempt - 1))).await;
            }
            match self.auth.apply(build()).send().await {
                Ok(response) => {
                    let status = response.status();
                    if matches!(status.as_u16(), 429 | 502 | 503 | 504) && attempt + 1 < ATTEMPTS {
                        last = DavError::Status(status.as_u16());
                        continue;
                    }
                    return Ok(response);
                }
                Err(error) => {
                    last = DavError::from_reqwest(&error);
                    if last != DavError::Offline {
                        return Err(last);
                    }
                }
            }
        }
        Err(last)
    }

    fn check(response: &Response) -> Result<(), DavError> {
        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(DavError::Unauthorized),
            StatusCode::NOT_FOUND | StatusCode::GONE => Err(DavError::NotFound),
            status => Err(DavError::Status(status.as_u16())),
        }
    }

    /// The direct children of a folder (`PROPFIND Depth: 1`).
    pub async fn list(&self, path: &str) -> Result<Vec<Entry>, DavError> {
        let folder = if path.ends_with('/') { path.to_string() } else { format!("{path}/") };
        let url = self.url_for(&folder);
        let method = Method::from_bytes(b"PROPFIND").expect("method");
        let response = self
            .send(|| {
                self.http
                    .request(method.clone(), url.clone())
                    .timeout(LISTING_TIMEOUT)
                    .header("Depth", "1")
                    .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
                    .body(PROPFIND_BODY)
            })
            .await?;
        match response.status().as_u16() {
            207 => {}
            // A plain web server answers a PROPFIND with 405 or a page.
            200 | 400 | 405 | 501 => return Err(DavError::NotWebdav),
            _ => Self::check(&response).and(Err(DavError::NotWebdav))?,
        }
        let body = read_capped(response, MAX_LISTING_BYTES as u64).await?;
        let text = String::from_utf8_lossy(&body);
        propfind::parse(&text, &folder).map_err(|_| DavError::NotWebdav)
    }

    /// A whole file of at most `max` bytes (a `.mokuro`, a cover).
    pub async fn get(&self, path: &str, max: u64) -> Result<Vec<u8>, DavError> {
        let url = self.url_for(path);
        let response = self.send(|| self.http.get(url.clone()).timeout(FILE_TIMEOUT)).await?;
        Self::check(&response)?;
        read_capped(response, max).await
    }

    /// `len` bytes from `start`. `total` is the file's size as listed: a server
    /// that reports another size has a different file now. A server that
    /// ignores `Range` and starts sending the whole file is cut off at once.
    pub async fn range(&self, path: &str, start: u64, len: u64, total: Option<u64>) -> Result<Vec<u8>, DavError> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let url = self.url_for(path);
        let end = start + len - 1;
        let response = self
            .send(|| self.http.get(url.clone()).timeout(FILE_TIMEOUT).header(header::RANGE, format!("bytes={start}-{end}")))
            .await?;
        match response.status() {
            StatusCode::PARTIAL_CONTENT => {}
            // Dropping the response closes it before the body comes.
            StatusCode::OK => return Err(DavError::NoRange),
            StatusCode::RANGE_NOT_SATISFIABLE => return Err(DavError::Changed),
            _ => Self::check(&response).and(Err(DavError::NoRange))?,
        }
        let content_range = response.headers().get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok()).map(str::to_string);
        match content_range.as_deref().and_then(parse_content_range) {
            Some((from, to, size)) => {
                if from != start || to != end {
                    return Err(DavError::NoRange);
                }
                if let (Some(size), Some(total)) = (size, total) {
                    if size != total {
                        return Err(DavError::Changed);
                    }
                }
            }
            None => return Err(DavError::NoRange),
        }
        let body = read_capped(response, len).await?;
        if body.len() as u64 != len {
            return Err(DavError::Other("short range".into()));
        }
        Ok(body)
    }

    /// The size of a file, for when a listing did not say.
    pub async fn size(&self, path: &str) -> Result<Option<u64>, DavError> {
        let url = self.url_for(path);
        let response = self.send(|| self.http.head(url.clone()).timeout(LISTING_TIMEOUT)).await?;
        Self::check(&response)?;
        Ok(response.content_length().filter(|&len| len > 0))
    }

    /// The whole file into `dest`, through `dest.part` so a half download is
    /// never taken for a file. `progress` is called with what arrived so far
    /// and the total, if known; `cancel` stops it between chunks.
    pub async fn download(
        &self,
        path: &str,
        dest: &Path,
        mut progress: impl FnMut(u64, Option<u64>),
        cancel: &AtomicBool,
    ) -> Result<u64, DavError> {
        use std::io::Write;
        let url = self.url_for(path);
        let mut response = self.send(|| self.http.get(url.clone())).await?;
        Self::check(&response)?;
        let total = response.content_length();
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| DavError::Other(e.to_string()))?;
        }
        let partial = dest.with_extension("part");
        let result = async {
            let mut file = std::io::BufWriter::new(std::fs::File::create(&partial).map_err(|e| DavError::Other(e.to_string()))?);
            let mut received = 0u64;
            progress(0, total);
            while let Some(chunk) = response.chunk().await.map_err(|e| DavError::from_reqwest(&e))? {
                if cancel.load(Ordering::Relaxed) {
                    return Err(DavError::Cancelled);
                }
                file.write_all(&chunk).map_err(|e| DavError::Other(e.to_string()))?;
                received += chunk.len() as u64;
                progress(received, total);
            }
            file.flush().map_err(|e| DavError::Other(e.to_string()))?;
            if total.is_some_and(|total| total != received) {
                return Err(DavError::Offline);
            }
            Ok(received)
        }
        .await;
        match result {
            Ok(received) => {
                std::fs::rename(&partial, dest).map_err(|e| DavError::Other(e.to_string()))?;
                Ok(received)
            }
            Err(error) => {
                let _ = std::fs::remove_file(&partial);
                Err(error)
            }
        }
    }
}

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/><d:getlastmodified/><d:getetag/></d:prop></d:propfind>"#;

/// `bytes 0-99/1234` → `(0, 99, Some(1234))`; `*` for an unknown size.
fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (span, size) = rest.split_once('/')?;
    let (from, to) = span.split_once('-')?;
    let size = match size.trim() {
        "*" => None,
        size => Some(size.parse().ok()?),
    };
    Some((from.trim().parse().ok()?, to.trim().parse().ok()?, size))
}

/// A body, refused as soon as it passes `max`.
async fn read_capped(mut response: Response, max: u64) -> Result<Vec<u8>, DavError> {
    if response.content_length().is_some_and(|len| len > max) {
        return Err(DavError::TooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| DavError::from_reqwest(&e))? {
        if body.len() as u64 + chunk.len() as u64 > max {
            return Err(DavError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::webdav::test_server::{Reply, TestServer};

    fn run<F: std::future::Future>(future: F) -> F::Output {
        tauri::async_runtime::block_on(future)
    }

    #[test]
    fn addresses_are_cleaned_and_credentials_taken_out() {
        let (url, auth) = parse_base_url("https://yuki:p%40ss@files.example/dav?x=1#y").unwrap();
        assert_eq!(url.as_str(), "https://files.example/dav/");
        assert_eq!(auth, Some(Auth::Basic { username: "yuki".into(), password: "p@ss".into() }));
        assert_eq!(parse_base_url("http://h.example:8080").unwrap().0.as_str(), "http://h.example:8080/");
        for bad in ["ftp://h.example/", "file:///etc", "not a url", "https://"] {
            assert_eq!(parse_base_url(bad).unwrap_err(), DavError::InvalidUrl, "{bad}");
        }
    }

    #[test]
    fn paths_are_encoded_by_segment_and_stay_on_the_server() {
        let client = Client::new(parse_base_url("https://h.example/dav/").unwrap().0, Auth::None);
        assert_eq!(client.base_path(), "/dav/");
        assert_eq!(client.url_for("/dav/星 (1)/a#b?.cbz").as_str(), "https://h.example/dav/%E6%98%9F%20%281%29/a%23b%3F.cbz");
        assert_eq!(client.url_for("//evil.example/x").host_str(), Some("h.example"));
    }

    #[test]
    fn auth_is_never_printed_and_round_trips_through_json() {
        let auth = Auth::Basic { username: "yuki".into(), password: "hunter2".into() };
        assert_eq!(format!("{auth:?}"), "Auth::basic");
        let json = auth.to_json().unwrap();
        assert_eq!(Auth::from_json(Some(&json)), auth);
        assert_eq!(Auth::from_json(Some("garbage")), Auth::None);
        assert_eq!(Auth::None.to_json(), None);
    }

    #[test]
    fn content_ranges_are_read_strictly() {
        assert_eq!(parse_content_range("bytes 0-99/1234"), Some((0, 99, Some(1234))));
        assert_eq!(parse_content_range("bytes 5-9/*"), Some((5, 9, None)));
        assert_eq!(parse_content_range("items 0-1/2"), None);
    }

    #[test]
    fn listings_send_the_credentials_and_map_errors() {
        let server = TestServer::start(|request| {
            let authorized = request.header("authorization") == Some("Basic eXVraTpodW50ZXIy");
            match (request.method.as_str(), request.path.as_str()) {
                (_, _) if !authorized => Reply::status(401),
                ("PROPFIND", "/mokuro-reader/Hoshi%20%28Test%29/") if request.header("depth") == Some("1") => {
                    Reply::new(207, crate::manga::webdav::propfind::tests::BUNKO_SERIES.as_bytes().to_vec())
                }
                ("PROPFIND", "/web/") => Reply::new(405, Vec::new()),
                _ => Reply::status(404),
            }
        });
        let base = parse_base_url(&server.url("/")).unwrap().0;
        let good = Client::new(base.clone(), Auth::Basic { username: "yuki".into(), password: "hunter2".into() });
        let entries = run(good.list("/mokuro-reader/Hoshi (Test)/")).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(run(good.list("/web/")).unwrap_err(), DavError::NotWebdav);
        assert_eq!(run(good.list("/missing/")).unwrap_err(), DavError::NotFound);
        let wrong = Client::new(base, Auth::Basic { username: "yuki".into(), password: "nope".into() });
        assert_eq!(run(wrong.list("/mokuro-reader/Hoshi (Test)/")).unwrap_err(), DavError::Unauthorized);
    }

    #[test]
    fn bearer_tokens_go_in_the_authorization_header() {
        let server = TestServer::start(|request| {
            if request.header("authorization") == Some("Bearer t0ken") { Reply::new(200, b"ok".to_vec()) } else { Reply::status(401) }
        });
        let client = Client::new(parse_base_url(&server.url("/")).unwrap().0, Auth::Bearer { token: "t0ken".into() });
        assert_eq!(run(client.get("/file.txt", 10)).unwrap(), b"ok");
        assert_eq!(run(client.get("/file.txt", 1)).unwrap_err(), DavError::TooLarge);
    }

    #[test]
    fn a_server_that_is_not_there_is_offline_after_retrying() {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let client = Client::new(parse_base_url(&format!("http://127.0.0.1:{port}/")).unwrap().0, Auth::None);
        assert_eq!(run(client.list("/")).unwrap_err(), DavError::Offline);
    }

    #[test]
    fn a_busy_server_is_asked_again() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = calls.clone();
        let server = TestServer::start(move |_| {
            if seen.fetch_add(1, Ordering::SeqCst) == 0 { Reply::status(503) } else { Reply::new(200, b"ok".to_vec()) }
        });
        let client = Client::new(parse_base_url(&server.url("/")).unwrap().0, Auth::None);
        assert_eq!(run(client.get("/x", 10)).unwrap(), b"ok");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
