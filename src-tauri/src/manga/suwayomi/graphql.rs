//! Talking to Suwayomi-Server: its GraphQL API (`POST /api/graphql`) and the
//! few REST endpoints that only serve bytes (extension icons, thumbnails,
//! chapter pages).
//!
//! Verified against Suwayomi-Server v2.4.2366 (its schema by introspection,
//! and `ExtensionStoreMutation.kt`, `ConfigModule.kt`, `JavalinSetup.kt` of
//! that tag):
//!
//! - `addExtensionStore(input: {indexUrl})` takes a `repo.json` (whose
//!   `index_v2` it follows) or the protobuf `index.pb` directly;
//! - `fetchExtensions(input: {})` → `extensions[]` with `contentWarning`
//!   (`SAFE`, `MIXED`, `NSFW`), `hasUpdate`, `isInstalled`, `isObsolete`,
//!   `iconUrl` (`/api/v1/extension/icon/<pkg>`);
//! - `updateExtension(input: {id: <pkgName>, patch: {install|update|uninstall}})`;
//! - `sources` (query) → `nodes[]` with `id` as a string (`LongString`);
//! - `fetchSourceManga(input: {source, type: POPULAR|LATEST|SEARCH, page, query})`
//!   → `{hasNextPage, mangas[]}`;
//! - `fetchMangaAndChapters(input: {id, fetchManga, fetchChapters})`
//!   (`fetchChapters` alone is deprecated) → `{manga, chapters[]}`;
//! - `fetchChapterPages(input: {chapterId})` → `pages[]`, server paths like
//!   `/api/v1/manga/2/chapter/1/page/0`.
//!
//! Errors come back as HTTP 200 with `errors[].message` (a Kotlin stack
//! trace): they are logged, never shown, and become `SwError::Server`.
//!
//! Every path the server hands us is checked by [`server_path`] before it is
//! asked for: only `/api/v1/…` on the same server, never another host.

use std::time::Duration;

use reqwest::{header, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::manga::webdav::client::Auth;

/// Why talking to the server failed, as the UI tells it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SwError {
    #[error("the server could not be reached")]
    Offline,
    #[error("the server refused the credentials")]
    Unauthorized,
    #[error("the managed server is not installed")]
    NotInstalled,
    #[error("no server is configured")]
    NotConfigured,
    #[error("the server did not start")]
    StartFailed,
    #[error("the server answered with an error")]
    Server,
    #[error("the server's answer was not understood")]
    BadReply,
    #[error("not found")]
    NotFound,
    #[error("could not write to disk")]
    Disk,
    #[error("no Java runtime for this computer")]
    Unsupported,
    #[error("already running")]
    Busy,
}

impl SwError {
    pub fn code(&self) -> &'static str {
        match self {
            SwError::Offline => "offline",
            SwError::Unauthorized => "unauthorized",
            SwError::NotInstalled => "not-installed",
            SwError::NotConfigured => "not-configured",
            SwError::StartFailed => "start-failed",
            SwError::Server => "server",
            SwError::BadReply => "bad-reply",
            SwError::NotFound => "not-found",
            SwError::Disk => "disk",
            SwError::Unsupported => "unsupported",
            SwError::Busy => "busy",
        }
    }
}

/// `SAFE`, `MIXED` or `NSFW`, as the store marks an extension and its
/// sources. Anything else is read as `NSFW`: unknown is never safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Content {
    Safe,
    Mixed,
    Nsfw,
}

impl Content {
    pub fn parse(raw: &str) -> Content {
        match raw {
            "SAFE" => Content::Safe,
            "MIXED" => Content::Mixed,
            _ => Content::Nsfw,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Content::Safe => "safe",
            Content::Mixed => "mixed",
            Content::Nsfw => "nsfw",
        }
    }

    /// Hidden unless the user asked for adult sources, and `is_adult` on a
    /// series added from it (so Discord never shows its title). A MIXED
    /// source can be either, so it is treated as adult: the safe side.
    pub fn adult(self) -> bool {
        self != Content::Safe
    }
}

impl<'de> Deserialize<'de> for Content {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Content::parse(&String::deserialize(deserializer)?))
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub pkg_name: String,
    pub name: String,
    pub lang: String,
    pub version_name: String,
    pub content_warning: Content,
    pub is_installed: bool,
    pub has_update: bool,
    #[serde(default)]
    pub is_obsolete: bool,
    pub icon_url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub lang: String,
    #[serde(default)]
    pub display_name: String,
    pub content_warning: Content,
    #[serde(default)]
    pub supports_latest: bool,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub extension: Option<SourceExtension>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceExtension {
    pub pkg_name: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manga {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub genre: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub source: Option<Source>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub chapter_number: f64,
    #[serde(default)]
    pub scanlator: Option<String>,
    /// Milliseconds since the epoch, as a string (`LongString`).
    #[serde(default)]
    pub upload_date: Option<String>,
    pub source_order: i64,
    #[serde(default)]
    pub page_count: i64,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowsePage {
    pub has_next_page: bool,
    pub mangas: Vec<Manga>,
}

/// POPULAR, LATEST or SEARCH (`FetchSourceMangaType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "ExtBrowseKind"))]
pub enum BrowseKind {
    Popular,
    Latest,
    Search,
}

impl BrowseKind {
    fn graphql(self) -> &'static str {
        match self {
            BrowseKind::Popular => "POPULAR",
            BrowseKind::Latest => "LATEST",
            BrowseKind::Search => "SEARCH",
        }
    }
}

/// What `updateExtension` is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "ExtAction"))]
pub enum ExtensionAction {
    Install,
    Update,
    Uninstall,
}

/// A path the server gave us, if it is one we will ask it for: on the same
/// server, under `/api/v1/`, plain characters, no `..`.
pub fn server_path(raw: &str) -> Option<String> {
    let ok = raw.starts_with("/api/v1/")
        && raw.len() <= 512
        && !raw.contains("..")
        && !raw.contains("//")
        && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._-?=&%".contains(&b));
    ok.then(|| raw.to_string())
}

/// `data.<field>` of a GraphQL answer, or why there is none.
pub fn parse_reply<T: DeserializeOwned>(body: &[u8], field: &str) -> Result<T, SwError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| SwError::BadReply)?;
    let data = value.get("data").and_then(|data| data.get(field)).filter(|data| !data.is_null());
    match data {
        Some(data) => serde_json::from_value(data.clone()).map_err(|error| {
            log::warn!("Suwayomi: unexpected shape of {field}: {error}");
            SwError::BadReply
        }),
        None => {
            if let Some(message) = value.get("errors").and_then(|errors| errors.get(0)).and_then(|error| error.get("message")).and_then(Value::as_str) {
                // A stack trace: for the log, not for the user.
                log::warn!("Suwayomi: {field} failed: {}", message.lines().next().unwrap_or(""));
            }
            Err(SwError::Server)
        }
    }
}

/// How long a request may take. Asking a source can mean the source's own
/// site is slow; installing an extension downloads it.
const QUICK: Duration = Duration::from_secs(15);
const SLOW: Duration = Duration::from_secs(90);

/// A server and how to get in.
#[derive(Clone)]
pub struct Server {
    base: Url,
    auth: Auth,
    http: reqwest::Client,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Neither the port of the managed server nor its credentials.
        write!(f, "Server({})", self.auth.kind())
    }
}

fn network(error: &reqwest::Error) -> SwError {
    if error.is_timeout() || error.is_connect() || error.is_request() {
        SwError::Offline
    } else {
        SwError::BadReply
    }
}

impl Server {
    /// `loopback`: the managed server, asked directly (never through a proxy
    /// the system may set).
    pub fn new(base: Url, auth: Auth, loopback: bool) -> Self {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("NihongoTrackerCompanion/", env!("CARGO_PKG_VERSION")))
            // A redirect could take the credentials elsewhere.
            .redirect(reqwest::redirect::Policy::none());
        if loopback {
            builder = builder.no_proxy();
        }
        Server { base, auth, http: builder.build().unwrap_or_default() }
    }

    fn url(&self, path: &str) -> Result<Url, SwError> {
        // The base may have a path of its own (a server behind a reverse
        // proxy at `/suwayomi/`).
        let prefix = self.base.path().trim_end_matches('/');
        let mut url = self.base.clone();
        let (path, query) = path.split_once('?').map_or((path, None), |(p, q)| (p, Some(q)));
        url.set_path(&format!("{prefix}{path}"));
        url.set_query(query);
        Ok(url)
    }

    async fn post(&self, body: Value, timeout: Duration) -> Result<Vec<u8>, SwError> {
        let request = self
            .http
            .post(self.url("/api/graphql")?)
            .timeout(timeout)
            .header(header::CONTENT_TYPE, "application/json")
            .json(&body);
        let response = self.auth.apply(request).send().await.map_err(|e| network(&e))?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(SwError::Unauthorized),
            status if status.is_server_error() => return Err(SwError::Server),
            status if !status.is_success() => return Err(SwError::BadReply),
            _ => {}
        }
        Ok(response.bytes().await.map_err(|e| network(&e))?.to_vec())
    }

    async fn run<T: DeserializeOwned>(&self, query: &str, variables: Value, field: &str, timeout: Duration) -> Result<T, SwError> {
        let body = self.post(json!({ "query": query, "variables": variables }), timeout).await?;
        parse_reply(&body, field)
    }

    /// The server's version: also the "is it up?" question.
    pub async fn version(&self) -> Result<String, SwError> {
        #[derive(Deserialize)]
        struct About {
            version: String,
        }
        let about: About = self.run("query { aboutServer { version } }", json!({}), "aboutServer", QUICK).await?;
        Ok(about.version)
    }

    /// The extension stores the server knows (their index URLs).
    pub async fn stores(&self) -> Result<Vec<String>, SwError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Store {
            index_url: String,
        }
        #[derive(Deserialize)]
        struct Nodes {
            nodes: Vec<Store>,
        }
        let list: Nodes = self.run("query { extensionStores { nodes { indexUrl } } }", json!({}), "extensionStores", QUICK).await?;
        Ok(list.nodes.into_iter().map(|store| store.index_url).collect())
    }

    pub async fn add_store(&self, index_url: &str) -> Result<(), SwError> {
        let _: Value = self
            .run(
                "mutation($url: String!) { addExtensionStore(input: {indexUrl: $url}) { extensionStore { indexUrl } } }",
                json!({ "url": index_url }),
                "addExtensionStore",
                SLOW,
            )
            .await?;
        Ok(())
    }

    /// Every extension of every store, refreshed from the stores.
    pub async fn fetch_extensions(&self) -> Result<Vec<Extension>, SwError> {
        #[derive(Deserialize)]
        struct Payload {
            extensions: Vec<Extension>,
        }
        let payload: Payload = self
            .run(
                "mutation { fetchExtensions(input: {}) { extensions { pkgName name lang versionName contentWarning isInstalled hasUpdate isObsolete iconUrl } } }",
                json!({}),
                "fetchExtensions",
                SLOW,
            )
            .await?;
        Ok(payload.extensions)
    }

    pub async fn update_extension(&self, pkg_name: &str, action: ExtensionAction) -> Result<Option<Extension>, SwError> {
        #[derive(Deserialize)]
        struct Payload {
            extension: Option<Extension>,
        }
        let patch = match action {
            ExtensionAction::Install => json!({ "install": true }),
            ExtensionAction::Update => json!({ "update": true }),
            ExtensionAction::Uninstall => json!({ "uninstall": true }),
        };
        let payload: Payload = self
            .run(
                "mutation($id: String!, $patch: UpdateExtensionPatchInput!) { updateExtension(input: {id: $id, patch: $patch}) { extension { pkgName name lang versionName contentWarning isInstalled hasUpdate isObsolete iconUrl } } }",
                json!({ "id": pkg_name, "patch": patch }),
                "updateExtension",
                SLOW,
            )
            .await?;
        Ok(payload.extension)
    }

    /// The sources of the installed extensions.
    pub async fn sources(&self) -> Result<Vec<Source>, SwError> {
        #[derive(Deserialize)]
        struct Nodes {
            nodes: Vec<Source>,
        }
        let list: Nodes = self
            .run(
                "query { sources { nodes { id name lang displayName contentWarning supportsLatest iconUrl extension { pkgName } } } }",
                json!({}),
                "sources",
                QUICK,
            )
            .await?;
        Ok(list.nodes)
    }

    pub async fn source(&self, id: &str) -> Result<Source, SwError> {
        self.run(
            "query($id: LongString!) { source(id: $id) { id name lang displayName contentWarning supportsLatest iconUrl extension { pkgName } } }",
            json!({ "id": id }),
            "source",
            QUICK,
        )
        .await
    }

    /// A page of a source's catalogue (1-based).
    pub async fn browse(&self, source: &str, kind: BrowseKind, page: u32, query: Option<&str>) -> Result<BrowsePage, SwError> {
        self.run(
            "mutation($source: LongString!, $type: FetchSourceMangaType!, $page: Int!, $query: String) { fetchSourceManga(input: {source: $source, type: $type, page: $page, query: $query}) { hasNextPage mangas { id title thumbnailUrl } } }",
            json!({ "source": source, "type": kind.graphql(), "page": page.max(1), "query": query }),
            "fetchSourceManga",
            SLOW,
        )
        .await
    }

    /// A series and its chapters. `refresh` asks the source again; without it
    /// the server answers with what it already has (and asks the source only
    /// if it has nothing).
    pub async fn manga(&self, id: i64, refresh: bool) -> Result<(Manga, Vec<Chapter>), SwError> {
        #[derive(Deserialize)]
        struct Payload {
            manga: Manga,
            chapters: Vec<Chapter>,
        }
        const FIELDS: &str = "id title thumbnailUrl author artist description genre status sourceId source { id name lang displayName contentWarning supportsLatest iconUrl }";
        const CHAPTER: &str = "id name chapterNumber scanlator uploadDate sourceOrder pageCount url";
        if !refresh {
            #[derive(Deserialize)]
            struct Stored {
                #[serde(flatten)]
                manga: Manga,
                chapters: Nodes,
            }
            #[derive(Deserialize)]
            struct Nodes {
                nodes: Vec<Chapter>,
            }
            let stored: Stored = self
                .run(&format!("query($id: Int!) {{ manga(id: $id) {{ {FIELDS} chapters {{ nodes {{ {CHAPTER} }} }} }} }}"), json!({ "id": id }), "manga", QUICK)
                .await?;
            if !stored.chapters.nodes.is_empty() {
                return Ok((stored.manga, stored.chapters.nodes));
            }
        }
        let payload: Payload = self
            .run(
                &format!("mutation($id: Int!) {{ fetchMangaAndChapters(input: {{id: $id, fetchManga: true, fetchChapters: true}}) {{ manga {{ {FIELDS} }} chapters {{ {CHAPTER} }} }} }}"),
                json!({ "id": id }),
                "fetchMangaAndChapters",
                SLOW,
            )
            .await?;
        Ok((payload.manga, payload.chapters))
    }

    /// The server paths of a chapter's pages. Anything that is not one of
    /// ours (see [`server_path`]) makes the whole answer unusable.
    pub async fn chapter_pages(&self, chapter_id: i64) -> Result<Vec<String>, SwError> {
        #[derive(Deserialize)]
        struct Payload {
            pages: Vec<String>,
        }
        let payload: Payload = self
            .run(
                "mutation($id: Int!) { fetchChapterPages(input: {chapterId: $id}) { pages } }",
                json!({ "id": chapter_id }),
                "fetchChapterPages",
                SLOW,
            )
            .await?;
        checked_pages(payload.pages)
    }

    /// Keeps the series in the server's own library, so it is not cleaned up.
    pub async fn set_in_library(&self, manga_id: i64, in_library: bool) -> Result<(), SwError> {
        let _: Value = self
            .run(
                "mutation($id: Int!, $in: Boolean!) { updateManga(input: {id: $id, patch: {inLibrary: $in}}) { manga { id } } }",
                json!({ "id": manga_id, "in": in_library }),
                "updateManga",
                QUICK,
            )
            .await?;
        Ok(())
    }

    /// The bytes behind a server path (an icon, a thumbnail, a page), at most
    /// `max` of them.
    pub async fn bytes(&self, path: &str, max: u64) -> Result<Vec<u8>, SwError> {
        let path = server_path(path).ok_or(SwError::NotFound)?;
        let request = self.http.get(self.url(&path)?).timeout(SLOW);
        let mut response = self.auth.apply(request).send().await.map_err(|e| network(&e))?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(SwError::Unauthorized),
            StatusCode::NOT_FOUND => return Err(SwError::NotFound),
            status if !status.is_success() => return Err(SwError::Server),
            _ => {}
        }
        if response.content_length().is_some_and(|len| len > max) {
            return Err(SwError::BadReply);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| network(&e))? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() as u64 > max {
                return Err(SwError::BadReply);
            }
        }
        Ok(bytes)
    }
}

fn checked_pages(pages: Vec<String>) -> Result<Vec<String>, SwError> {
    pages.iter().map(|page| server_path(page).ok_or(SwError::BadReply)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures written by hand with the shape Suwayomi-Server v2.4.2366
    // answers (checked against a running server).

    #[test]
    fn extensions_parse_with_their_content_warning() {
        let body = br#"{"data":{"fetchExtensions":{"extensions":[
            {"pkgName":"eu.kanade.tachiyomi.extension.ja.example","name":"Example","lang":"ja","versionName":"1.6.2",
             "contentWarning":"SAFE","isInstalled":false,"hasUpdate":false,"isObsolete":false,
             "iconUrl":"/api/v1/extension/icon/eu.kanade.tachiyomi.extension.ja.example"},
            {"pkgName":"eu.kanade.tachiyomi.extension.ja.other","name":"Other","lang":"ja","versionName":"1.6.20",
             "contentWarning":"MIXED","isInstalled":true,"hasUpdate":true,"isObsolete":false,"iconUrl":"/api/v1/extension/icon/x"},
            {"pkgName":"p","name":"N","lang":"all","versionName":"1","contentWarning":"SOMETHING_NEW","isInstalled":false,
             "hasUpdate":false,"iconUrl":"/api/v1/extension/icon/p"}
        ]}}}"#;
        #[derive(Deserialize)]
        struct Payload {
            extensions: Vec<Extension>,
        }
        let payload: Payload = parse_reply(body, "fetchExtensions").unwrap();
        let list = payload.extensions;
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].content_warning, Content::Safe);
        assert_eq!(list[1].content_warning, Content::Mixed);
        assert!(list[1].has_update && list[1].is_installed);
        // An unknown rating is never safe.
        assert_eq!(list[2].content_warning, Content::Nsfw);
        assert!(!Content::Safe.adult() && Content::Mixed.adult() && Content::Nsfw.adult());
    }

    #[test]
    fn errors_are_server_errors_and_bad_shapes_are_bad_replies() {
        let failed = br#"{"data":{"addExtensionStore":null},"errors":[{"message":"Exception while fetching data (/addExtensionStore) : HTTP error 403\r\n\tat suwayomi...","path":["addExtensionStore"]}]}"#;
        assert_eq!(parse_reply::<Value>(failed, "addExtensionStore").unwrap_err(), SwError::Server);
        assert_eq!(parse_reply::<Value>(b"<html>", "x").unwrap_err(), SwError::BadReply);
        let wrong = br#"{"data":{"fetchSourceManga":{"hasNextPage":"yes","mangas":[]}}}"#;
        assert_eq!(parse_reply::<BrowsePage>(wrong, "fetchSourceManga").unwrap_err(), SwError::BadReply);
    }

    #[test]
    fn a_catalogue_page_and_a_series_with_chapters_parse() {
        let page = r#"{"data":{"fetchSourceManga":{"hasNextPage":true,"mangas":[
            {"id":2,"title":"終活","thumbnailUrl":"/api/v1/manga/2/thumbnail"},
            {"id":3,"title":"B","thumbnailUrl":null}]}}}"#;
        let page: BrowsePage = parse_reply(page.as_bytes(), "fetchSourceManga").unwrap();
        assert!(page.has_next_page);
        assert_eq!(page.mangas[0].id, 2);
        assert_eq!(page.mangas[1].thumbnail_url, None);

        let series = r#"{"data":{"fetchMangaAndChapters":{
            "manga":{"id":2,"title":"T","thumbnailUrl":"/api/v1/manga/2/thumbnail","author":"A","artist":null,
                     "description":"D","genre":["x"],"status":"UNKNOWN","sourceId":"5332604811772266259",
                     "source":{"id":"5332604811772266259","name":"S","lang":"ja","displayName":"S (JA)","contentWarning":"NSFW",
                               "supportsLatest":true,"iconUrl":"/api/v1/extension/icon/p"}},
            "chapters":[{"id":176,"name":"第１話","chapterNumber":-1.0,"scanlator":null,"uploadDate":"1541689200000",
                         "sourceOrder":1,"pageCount":-1,"url":"123"}]}}}"#;
        #[derive(Deserialize)]
        struct Payload {
            manga: Manga,
            chapters: Vec<Chapter>,
        }
        let payload: Payload = parse_reply(series.as_bytes(), "fetchMangaAndChapters").unwrap();
        assert_eq!(payload.manga.source.unwrap().content_warning, Content::Nsfw);
        assert_eq!(payload.chapters[0].source_order, 1);
        assert_eq!(payload.chapters[0].upload_date.as_deref(), Some("1541689200000"));
    }

    #[test]
    fn page_paths_must_stay_on_the_server() {
        let ok = vec!["/api/v1/manga/2/chapter/1/page/0".to_string(), "/api/v1/manga/2/chapter/1/page/1".to_string()];
        assert_eq!(checked_pages(ok.clone()).unwrap(), ok);
        for bad in ["https://evil.example/x.jpg", "//evil.example/api/v1/x", "/api/v1/../../etc/passwd", "/api/v2/x", "/api/v1/x y", "/api/graphql"] {
            assert!(server_path(bad).is_none(), "{bad}");
            assert_eq!(checked_pages(vec![bad.to_string()]).unwrap_err(), SwError::BadReply);
        }
    }

    #[test]
    fn urls_keep_the_base_path_and_never_show_credentials() {
        let server = Server::new(
            Url::parse("https://example.org/suwayomi/").unwrap(),
            Auth::Basic { username: "user".into(), password: "hunter2".into() },
            false,
        );
        assert_eq!(server.url("/api/v1/manga/2/thumbnail").unwrap().as_str(), "https://example.org/suwayomi/api/v1/manga/2/thumbnail");
        assert_eq!(server.url("/api/graphql").unwrap().as_str(), "https://example.org/suwayomi/api/graphql");
        let shown = format!("{server:?}");
        assert!(!shown.contains("hunter2") && !shown.contains("user") && !shown.contains("example.org"));
    }
}
