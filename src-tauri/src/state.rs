use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
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
    /// Sub-second remainder of `elapsed_secs`, so ticks that are not a whole
    /// second long add up to real time instead of being rounded each time.
    #[serde(skip)]
    pub elapsed_carry_ms: u64,
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

/// Adds real time to a whole-second counter, keeping the remainder in `carry_ms`.
fn credit_elapsed(secs: &mut u64, carry_ms: &mut u64, delta: Duration) {
    let total = *carry_ms + delta.as_millis() as u64;
    *secs += total / 1000;
    *carry_ms = total % 1000;
}

impl AnimeSession {
    pub fn credit_elapsed(&mut self, delta: Duration) {
        credit_elapsed(&mut self.elapsed_secs, &mut self.elapsed_carry_ms, delta);
    }

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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct VnSession {
    pub exe: String,
    /// Concrete process instance selected by the detector.
    #[serde(default)]
    pub pid: u32,
    pub content_id: String,
    pub media_title: String,
    pub media_type: String,
    pub image: Option<String>,
    /// Carried from the registry so the presence can be stripped without a
    /// round trip to the API.
    #[serde(default)]
    pub is_adult: bool,
    #[serde(default)]
    pub is_adult_image: Option<bool>,
    pub elapsed_secs: u64,
    /// See [`AnimeSession::elapsed_carry_ms`].
    #[serde(skip)]
    pub elapsed_carry_ms: u64,
    /// Whether time is being counted at this instant.
    pub counting: bool,
    #[serde(default)]
    pub paused_by_user: bool,
    pub started_at: String,
    /// Characters seen by the texthooker since the session started.
    pub session_chars: i64,
    pub session_lines: i64,
}

impl VnSession {
    pub fn credit_elapsed(&mut self, delta: Duration) {
        credit_elapsed(&mut self.elapsed_secs, &mut self.elapsed_carry_ms, delta);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct TrackerSnapshot {
    pub platform_supported: bool,
    /// What detection can see on this OS and session (see `platform`).
    #[serde(default)]
    pub capabilities: crate::platform::Capabilities,
    pub enabled: bool,
    pub active_window: Option<String>,
    pub active_process: Option<String>,
    pub idle_seconds: u64,
    pub anime: Option<AnimeSession>,
    pub vn: Option<VnSession>,
    pub queued_count: i64,
    pub pending_count: i64,
    #[serde(default)]
    pub game_hook: NativeGameHookStatus,
}

pub struct VnTimerEdit {
    pub pid: u32,
    pub content_id: String,
    pub started_at: String,
    pub seconds: u64,
}

impl VnTimerEdit {
    pub fn apply(&self, session: &mut VnSession) -> bool {
        if session.pid != self.pid || session.content_id != self.content_id || session.started_at != self.started_at {
            return false;
        }
        session.elapsed_secs = self.seconds;
        session.elapsed_carry_ms = 0;
        true
    }
}

#[cfg(test)]
mod timer_tests {
    use super::*;

    #[test]
    fn timer_edits_reset_and_set_only_the_same_launch_without_changing_text_counts() {
        let mut session: VnSession = serde_json::from_value(serde_json::json!({
            "exe":"game.exe", "pid":42, "content_id":"v1", "media_title":"VN", "media_type":"vn",
            "elapsed_secs":120, "counting":true, "started_at":"launch", "session_chars":80, "session_lines":2
        })).unwrap();
        let mut edit = VnTimerEdit { pid: 42, content_id: "v1".into(), started_at: "launch".into(), seconds: 0 };
        assert!(edit.apply(&mut session));
        assert_eq!(session.elapsed_secs, 0);
        edit.seconds = 180;
        assert!(edit.apply(&mut session));
        assert_eq!(session.elapsed_secs, 180);
        assert_eq!((session.session_chars, session.session_lines, session.counting), (80, 2, true));
        session.started_at = "next launch".into();
        edit.seconds = 0;
        assert!(!edit.apply(&mut session));
        assert_eq!(session.elapsed_secs, 180);
        session.started_at = "launch".into();
        session.content_id = "v2".into();
        assert!(!edit.apply(&mut session));
        session.content_id = "v1".into();
        session.pid = 43;
        assert!(!edit.apply(&mut session));
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum NativeGameHookPhase {
    #[default]
    Inactive,
    Starting,
    Attached,
    Receiving,
    Error,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct NativeGameHookStatus {
    pub phase: NativeGameHookPhase,
    pub content_id: Option<String>,
    pub exe: Option<String>,
    pub pid: Option<u32>,
    pub message: Option<String>,
    #[serde(default)]
    pub hooks: Vec<NativeHookCandidate>,
    #[serde(default)]
    pub selected_hook_code: Option<String>,
    #[serde(default)]
    pub websocket_selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct NativeHookCandidate {
    pub selection_key: String,
    pub hook_code: String,
    pub name: String,
    pub sample: Option<String>,
}

impl NativeGameHookStatus {
    pub fn is_active_for(&self, content_id: Option<&str>) -> bool {
        !self.websocket_selected && self.content_id.is_some() && self.content_id.as_deref() == content_id
            && matches!(self.phase, NativeGameHookPhase::Starting | NativeGameHookPhase::Attached | NativeGameHookPhase::Receiving)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum HookerStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct HookerState {
    pub status: HookerStatus,
    pub url: String,
    /// Session shown in the reader and used by socket and clipboard input.
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
    pub reading_overlay: std::sync::Mutex<crate::reading_overlay::Runtime>,
    pub db: Arc<Db>,
    pub dictionaries: Arc<crate::dictionary::DictionaryStore>,
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
    /// Manual pause for the currently detected novel or game session.
    pub session_paused: AtomicBool,
    pub vn_timer_edit: std::sync::Mutex<Option<VnTimerEdit>>,
    /// Last streak answer, shared by the reminder loop and the UI so the two of
    /// them together still respect one request per five minutes.
    pub streak: RwLock<StreakCache>,
    /// Set when `auth/verify` answers 401. Stops the reminder loop instead of
    /// retrying a revoked key all day; cleared when the settings are saved.
    pub streak_auth_failed: AtomicBool,
    /// The Discord Rich Presence thread, present only while the user has the
    /// feature switched on. `None` means no socket has ever been touched.
    pub discord: RwLock<Option<crate::discord::Discord>>,
    pub game_hook: crate::native_hook::NativeGameHookManager,
    pub text_cache_tasks: RwLock<std::collections::HashSet<String>>,
    pub connection_online: AtomicBool,
    pub browser_bridge: RwLock<Option<crate::native_bridge::BrowserBridge>>,
    /// Held for a whole outbox drain. The backend has no idempotency key, so
    /// two drains reading the same `queued` rows would post every log twice.
    pub outbox_flush: tokio::sync::Mutex<()>,
    /// What the main window should do once its page loads, when it had to be
    /// built for a tray click or a notification. Taken by `take_main_intent`.
    pub main_intent: std::sync::Mutex<Option<String>>,
    /// Last `auth/verify` outcome and when it was taken, so every window that
    /// opens does not send its own request (see `connection::cached`).
    pub connection_last: std::sync::Mutex<Option<(std::time::Instant, serde_json::Value)>>,
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
