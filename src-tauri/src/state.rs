use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

use crate::api::{ApiClient, MediaHit};
use crate::config::Settings;
use crate::db::Db;
use crate::parser::ParsedAnime;

/// Media the tracker managed to resolve for a detected title.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMatch {
    pub content_id: String,
    pub title: String,
    pub media_type: String,
    pub image: Option<String>,
    pub episode_duration: Option<u32>,
    /// `true` when the user pinned this match by hand.
    pub manual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimeSession {
    /// Stable identity for the episode currently on screen.
    pub key: String,
    pub player: String,
    pub file_name: String,
    pub parsed: ParsedAnime,
    pub media: Option<MediaMatch>,
    /// `false` while the media lookup is still in flight.
    pub resolved: bool,
    /// Set when the API search came back empty.
    pub unmatched: bool,
    pub elapsed_secs: u64,
    pub threshold_secs: u64,
    pub logged: bool,
    pub started_at: String,
}

impl AnimeSession {
    pub fn ready(&self) -> bool {
        self.elapsed_secs >= self.threshold_secs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VnSession {
    pub exe: String,
    pub content_id: String,
    pub media_title: String,
    pub media_type: String,
    pub image: Option<String>,
    pub elapsed_secs: u64,
    /// Whether time is being counted at this instant.
    pub counting: bool,
    pub started_at: String,
    /// Characters seen by the texthooker since the session started.
    pub session_chars: i64,
    pub session_lines: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackerSnapshot {
    pub platform_supported: bool,
    pub enabled: bool,
    pub active_window: Option<String>,
    pub active_process: Option<String>,
    pub idle_seconds: u64,
    pub anime: Option<AnimeSession>,
    pub vn: Option<VnSession>,
    pub queued_count: i64,
    pub pending_count: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookerStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookerState {
    pub status: HookerStatus,
    pub url: String,
    /// Session the incoming lines are attributed to (`v17`, `session-<uuid>`…).
    pub content_id: Option<String>,
    pub content_title: Option<String>,
    pub last_error: Option<String>,
}

impl Default for HookerState {
    fn default() -> Self {
        Self {
            status: HookerStatus::Disconnected,
            url: String::new(),
            content_id: None,
            content_title: None,
            last_error: None,
        }
    }
}

/// Commands accepted by the texthooker WebSocket task.
#[derive(Debug, Clone)]
pub enum HookerCommand {
    Connect(String),
    Disconnect,
}

pub struct AppState {
    pub db: Arc<Db>,
    pub settings: RwLock<Settings>,
    pub tracker: RwLock<TrackerSnapshot>,
    pub hooker: RwLock<HookerState>,
    pub hooker_tx: mpsc::UnboundedSender<HookerCommand>,
    /// title (lowercased) → resolved media, `None` when the search found nothing.
    pub media_cache: RwLock<HashMap<String, Option<MediaMatch>>>,
}

impl AppState {
    pub async fn client(&self) -> ApiClient {
        let s = self.settings.read().await;
        ApiClient::new(s.api_root(), s.api_key.clone())
    }

    pub async fn settings_snapshot(&self) -> Settings {
        self.settings.read().await.clone()
    }
}

impl From<MediaHit> for MediaMatch {
    fn from(hit: MediaHit) -> Self {
        MediaMatch {
            title: hit.title.preferred(),
            content_id: hit.content_id,
            media_type: hit.media_type.unwrap_or_else(|| "anime".into()),
            image: hit.content_image.or(hit.cover_image),
            episode_duration: hit.episode_duration,
            manual: false,
        }
    }
}
