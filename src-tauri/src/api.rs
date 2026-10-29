//! Thin client over the NihongoTracker REST API.
//!
//! Authenticates with `x-api-key` (see `Backend/src/middlewares/authMiddleware.ts`),
//! which the backend checks before the browser cookie — the only workable option
//! for a native client, since the JWT cookie is `httpOnly` + `sameSite: strict`.

use std::time::Duration;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One connection pool for every API call. Building a `reqwest::Client` sets up
/// a new pool and re-parses the TLS root store, so a client per request paid a
/// full TLS handshake every time. `ApiClient` is cheap to build on top of it.
static HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("NihongoTrackerCompanion/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest client")
});

/// For services on this machine (MPC's web interface, AnkiConnect): never
/// through a system proxy, and each caller sets its own short timeout.
static LOCAL_HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("reqwest client")
});

pub fn local_http() -> &'static reqwest::Client {
    &LOCAL_HTTP
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network error: {0}")]
    Network(String),
    #[error("{message}")]
    Http {
        status: u16,
        message: String,
        code: Option<String>,
    },
    #[error("not configured: missing API key")]
    NotConfigured,
}

impl ApiError {
    /// Retrying will never help for these.
    pub fn is_permanent(&self) -> bool {
        match self {
            ApiError::Http { status, .. } => (400..500).contains(status) && *status != 408 && *status != 429,
            ApiError::NotConfigured => false,
            ApiError::Network(_) => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaHit {
    #[serde(rename = "contentId")]
    pub content_id: String,
    #[serde(default)]
    pub title: MediaTitle,
    #[serde(rename = "contentImage")]
    pub content_image: Option<String>,
    #[serde(rename = "coverImage")]
    pub cover_image: Option<String>,
    #[serde(default, rename = "isAdult")]
    pub is_adult: bool,
    #[serde(default, rename = "isAdultImage")]
    pub is_adult_image: Option<bool>,
    #[serde(default)]
    pub synonyms: Option<Vec<String>>,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    #[serde(default)]
    pub episodes: Option<u32>,
    #[serde(default, rename = "episodeDuration")]
    pub episode_duration: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaTitle {
    #[serde(default, rename = "contentTitleNative")]
    pub native: String,
    #[serde(default, rename = "contentTitleRomaji")]
    pub romaji: Option<String>,
    #[serde(default, rename = "contentTitleEnglish")]
    pub english: Option<String>,
}

impl MediaTitle {
    pub fn preferred(&self) -> String {
        self.romaji
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| self.english.clone().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| self.native.clone())
    }
}

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    root: String,
    key: Option<String>,
}

impl ApiClient {
    pub fn new(root: String, key: Option<String>) -> Self {
        // A clone shares the pool; it is only a reference count.
        Self { http: HTTP.clone(), root, key }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.root.trim_end_matches('/'), path.trim_start_matches('/'))
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder> {
        let key = self.key.as_ref().ok_or(ApiError::NotConfigured)?;
        if key.trim().is_empty() {
            return Err(ApiError::NotConfigured);
        }
        Ok(req.header("x-api-key", key))
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value> {
        let res = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = res.status();
        let body = res.text().await.unwrap_or_default();

        if status.is_success() {
            if body.trim().is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_str(&body).map_err(|_| ApiError::Http {
                status: status.as_u16(),
                // A JSON parse failure here almost always means the SPA fallback
                // served index.html — i.e. the base URL is wrong.
                message: "Unexpected non-JSON response (check the server URL)".into(),
                code: None,
            });
        }

        let parsed: Option<Value> = serde_json::from_str(&body).ok();
        let mut message = parsed
            .as_ref()
            .and_then(|v| v.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Request failed")
            .to_string();
        // Validation errors name each failing path with mongoose's reason.
        if let Some(fields) = parsed.as_ref().and_then(|v| v.get("fields")).and_then(Value::as_object) {
            let reasons: Vec<&str> = fields.values().filter_map(Value::as_str).collect();
            if !reasons.is_empty() {
                message = format!("{message} — {}", reasons.join(" "));
            }
        }
        let code = parsed
            .as_ref()
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Err(ApiError::Http {
            status: status.as_u16(),
            message,
            code,
        })
    }

    // ── auth ───────────────────────────────────────────────────────────

    pub async fn verify(&self) -> Result<Value> {
        let req = self.auth(self.http.get(self.url("users/me")))?;
        self.send(req).await
    }

    // ── logs ───────────────────────────────────────────────────────────

    /// `key` is the outbox id: the backend answers a repeated key with the log
    /// it already created, so a retry after a lost response can't duplicate it.
    pub async fn create_log(&self, payload: &Value, key: &str) -> Result<Value> {
        let req = self.auth(
            self.http
                .post(self.url("logs"))
                .header("Idempotency-Key", key)
                .json(payload),
        )?;
        self.send(req).await
    }

    pub async fn preview_xp(&self, payload: &Value) -> Result<Value> {
        let req = self.auth(self.http.post(self.url("logs/preview-xp")).json(payload))?;
        self.send(req).await
    }

    pub async fn recent_logs(&self, username: &str, limit: u32) -> Result<Value> {
        let req = self.auth(
            self.http
                .get(self.url(&format!("users/{username}/logs")))
                .query(&[("limit", limit.to_string()), ("page", "1".into())]),
        )?;
        self.send(req).await
    }

    /// `GET /api/users/ranking` — the leaderboard the web app shows.
    pub async fn ranking(
        &self,
        page: u32,
        limit: u32,
        filter: &str,
        time_filter: &str,
        timezone: &str,
        audience: &str,
        medium_type: Option<&str>,
        medium_metric: Option<&str>,
    ) -> Result<Value> {
        let path = if medium_type.is_some() { "users/ranking/media" } else { "users/ranking" };
        let mut query = vec![
            ("page", page.to_string()),
            ("limit", limit.to_string()),
            ("filter", filter.to_string()),
            ("timeFilter", time_filter.to_string()),
            ("timezone", timezone.to_string()),
            ("audience", audience.to_string()),
        ];
        if let Some(media_type) = medium_type {
            query.push(("type", media_type.to_string()));
            query.push(("metric", medium_metric.unwrap_or("xp").to_string()));
        }
        let req = self.auth(self.http.get(self.url(path)).query(&query))?;
        self.send(req).await
    }

    /// `GET /api/activities` — authenticated social activity feed.
    pub async fn activity_feed(
        &self,
        scope: &str,
        before: Option<&str>,
        limit: u32,
    ) -> Result<Value> {
        let mut query = vec![
            ("scope", scope.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(cursor) = before.filter(|cursor| !cursor.trim().is_empty()) {
            query.push(("before", cursor.to_string()));
        }
        let req = self
            .auth(self.http.get(self.url("activities")).query(&query))?;
        self.send(req).await
    }

    pub async fn update_immersion_presence(&self, presence: &Value) -> Result<Value> {
        let req = self.auth(self.http.put(self.url("users/me/immersion-presence")).json(presence))?;
        self.send(req).await
    }

    pub async fn friends_immersion_presence(&self) -> Result<Value> {
        let req = self.auth(self.http.get(self.url("users/friends/immersion-presence")))?;
        self.send(req).await
    }

    pub async fn user_stats(&self, username: &str) -> Result<Value> {
        let req = self.auth(self.http.get(self.url(&format!("users/{username}/stats"))))?;
        self.send(req).await
    }

    // ── media ──────────────────────────────────────────────────────────

    /// `type` accepts the search spellings: anime, manga, reading, vn, movie,
    /// tv_show, game, book. Returns a bare array.
    pub async fn search_media(&self, search: &str, media_type: &str, per_page: u32) -> Result<Vec<MediaHit>> {
        let req = self.http.get(self.url("media/search")).query(&[
            ("search", search.to_string()),
            ("type", media_type.to_string()),
            ("page", "1".into()),
            ("perPage", per_page.to_string()),
        ]);
        let value = self.send(req).await?;
        Ok(serde_json::from_value(value).unwrap_or_default())
    }

    pub async fn multi_search(&self, search: &str, per_page: u32) -> Result<Vec<MediaHit>> {
        let req = self.http.get(self.url("media/multi-search")).query(&[
            ("search", search.to_string()),
            ("perPage", per_page.to_string()),
        ]);
        let value = self.send(req).await?;
        Ok(serde_json::from_value(value).unwrap_or_default())
    }

    pub async fn media_detail(&self, media_type: &str, content_id: &str) -> Result<Value> {
        let req = self
            .http
            .get(self.url(&format!("media/{media_type}/{content_id}")));
        self.send(req).await
    }

    // ── texthooker ─────────────────────────────────────────────────────

    pub async fn push_lines(&self, content_id: &str, lines: &Value) -> Result<Value> {
        let req = self.auth(
            self.http
                .post(self.url(&format!("texthooker/{content_id}/lines")))
                .json(&json!({ "lines": lines })),
        )?;
        self.send(req).await
    }

    pub async fn update_timer(&self, content_id: &str, seconds: i64) -> Result<Value> {
        let req = self.auth(
            self.http
                .patch(self.url(&format!("texthooker/{content_id}/timer")))
                .json(&json!({ "timerSeconds": seconds })),
        )?;
        self.send(req).await
    }

    pub async fn add_history(&self, content_id: &str, entry: &Value) -> Result<Value> {
        let req = self.auth(
            self.http
                .post(self.url(&format!("texthooker/{content_id}/history")))
                .json(entry),
        )?;
        self.send(req).await
    }

    pub async fn get_session(&self, content_id: &str) -> Result<Value> {
        let req = self.auth(self.http.get(self.url(&format!("texthooker/{content_id}"))))?;
        self.send(req).await
    }

    pub async fn media_stats(&self, content_id: &str, media_type: &str) -> Result<Value> {
        let req = self.auth(self.http.get(self.url("logs/stats/media"))
            .query(&[("mediaId", content_id), ("type", media_type)]))?;
        self.send(req).await
    }

    pub async fn cache_image(&self, url: &str) -> Option<String> {
        use base64::Engine;
        let url = reqwest::Url::parse(url).ok()?;
        if !matches!(url.scheme(), "https" | "http") { return None; }
        let mut response = self.http.get(url).send().await.ok()?.error_for_status().ok()?;
        let mime = response.headers().get(reqwest::header::CONTENT_TYPE)?.to_str().ok()?.split(';').next()?.to_string();
        if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "image/avif") { return None; }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if bytes.len() + chunk.len() > 5 * 1024 * 1024 { return None; }
            bytes.extend_from_slice(&chunk);
        }
        Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    }
}

/// Map an internal log/media type to the spelling `/media/search` expects.
pub fn search_type_for(log_type: &str) -> &str {
    match log_type {
        "tv show" => "tv_show",
        other => other,
    }
}
