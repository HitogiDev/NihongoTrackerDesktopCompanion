use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

use crate::api::{ApiClient, MediaHit};
use crate::config::Settings;
use crate::db::Db;
use crate::parser::ParsedAnime;
use crate::playback::{Playback, PlaybackSource};
use crate::streak::StreakCache;

/// Media the tracker managed to resolve for a detected title.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMatch {
    pub content_id: String,
    pub title: String,
    pub media_type: String,
    pub image: Option<String>,
    pub episode_duration: Option<u32>,
    /// `isAdult` from the media entry. Discord Rich Presence must never publish
    /// the title of a work flagged this way.
    #[serde(default)]
    pub is_adult: bool,
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
    /// Share of the episode that must be watched, kept per session so a settings
    /// change mid-episode doesn't move the goalposts.
    pub threshold_percent: u8,
    /// Where the player says it is, when it says (mpv IPC, MPC web interface).
    pub position_secs: Option<f64>,
    pub duration_secs: Option<f64>,
    pub paused: bool,
    pub source: PlaybackSource,
    /// 0.0–1.0 towards being logged; computed here so the UI never re-derives it.
    pub progress: f64,
    pub logged: bool,
    pub started_at: String,
}

impl AnimeSession {
    /// True once enough of the episode has been seen. Real playback position
    /// wins over wall-clock time whenever the player reports it — a paused
    /// player or a window left open otherwise counts as watching.
    pub fn ready(&self) -> bool {
        if let (Some(position), Some(duration)) = (self.position_secs, self.duration_secs) {
            if duration > 0.0 {
                return position / duration >= self.threshold_percent as f64 / 100.0;
            }
        }
        self.elapsed_secs >= self.threshold_secs
    }

    /// 0.0–1.0 towards being logged.
    pub fn compute_progress(&self) -> f64 {
        let ratio = match (self.position_secs, self.duration_secs) {
            (Some(position), Some(duration)) if duration > 0.0 => {
                (position / duration) / (self.threshold_percent.max(1) as f64 / 100.0)
            }
            _ if self.threshold_secs > 0 => self.elapsed_secs as f64 / self.threshold_secs as f64,
            _ => 1.0,
        };
        ratio.clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VnSession {
    pub exe: String,
    pub content_id: String,
    pub media_title: String,
    pub media_type: String,
    pub image: Option<String>,
    /// Carried from the registry so the presence can be stripped without a
    /// round trip to the API.
    #[serde(default)]
    pub is_adult: bool,
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

/// Last answer from the player, with the player it came from.
#[derive(Debug, Clone)]
pub struct PlaybackSnapshot {
    pub player: String,
    pub playback: Playback,
}

pub struct AppState {
    pub db: Arc<Db>,
    pub settings: RwLock<Settings>,
    pub tracker: RwLock<TrackerSnapshot>,
    pub hooker: RwLock<HookerState>,
    pub hooker_tx: mpsc::UnboundedSender<HookerCommand>,
    /// title (lowercased) → resolved media, `None` when the search found nothing.
    pub media_cache: RwLock<HashMap<String, Option<MediaMatch>>>,
    /// Filled by a task off the detection tick; one second stale at worst.
    pub playback: RwLock<Option<PlaybackSnapshot>>,
    /// Guard so a slow player can't pile up probes.
    pub playback_probing: AtomicBool,
    /// Whether the reader's timer is running. The UI owns it for manual
    /// sessions; ingestion checks it to honour the "during pause" settings.
    pub hooker_timer_active: AtomicBool,
    /// Last streak answer, shared by the reminder loop and the UI so the two of
    /// them together still respect one request per five minutes.
    pub streak: RwLock<StreakCache>,
    /// Set when `auth/verify` answers 401. Stops the reminder loop instead of
    /// retrying a revoked key all day; cleared when the settings are saved.
    pub streak_auth_failed: AtomicBool,
    /// The Discord Rich Presence thread, present only while the user has the
    /// feature switched on. `None` means no socket has ever been touched.
    pub discord: RwLock<Option<crate::discord::Discord>>,
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
            is_adult: hit.is_adult,
            manual: false,
        }
    }
}
