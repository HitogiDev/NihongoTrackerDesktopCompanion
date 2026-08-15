//! Optional Discord Rich Presence.
//!
//! Off by default and opt-in: nothing here runs, and no IPC socket is touched,
//! until the user turns it on. What gets published is decided by [`decide`],
//! which is a pure function precisely so the settings screen can show the user
//! the exact same thing Discord will show everyone else.
//!
//! Privacy rules, in the order they apply:
//! 1. Blacklisted media or process → nothing at all.
//! 2. `isAdult`, an unresolved media, or the global generic switch → generic
//!    presence: no title, no cover.
//! 3. Otherwise the full thing.
//!
//! The connection lives on its own OS thread because the crate's IPC is
//! blocking. Discord may not be running, may be started later, or may quit
//! mid-session; the thread reconnects with backoff and an IPC failure can never
//! reach the detection loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Settings;
use crate::db::BlacklistEntry;
use crate::state::{AppState, TrackerSnapshot};

pub const EVT_STATUS: &str = "discord://status";

/// Discord rate-limits presence updates to roughly one per 15 s. Updates that
/// arrive inside the window replace the pending one instead of queueing, so
/// what finally goes out is the newest state, never a backlog of stale ones.
const MIN_INTERVAL: Duration = Duration::from_secs(15);
const BACKOFF_START: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// How often the thread wakes to flush a throttled update or retry a connection.
const TICK: Duration = Duration::from_millis(500);

/// Exactly what the outside world would see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Presence {
    pub details: String,
    pub state: String,
    /// Cover URL or an asset key uploaded to the Discord application.
    #[serde(rename = "largeImage")]
    pub large_image: String,
    #[serde(rename = "largeText")]
    pub large_text: String,
    /// Session start, in epoch seconds. Discord counts the time itself — the
    /// app never pushes a ticking counter.
    #[serde(rename = "startEpoch")]
    pub start_epoch: i64,
    pub button: Option<PresenceButton>,
    /// True when every identifying detail was stripped.
    pub generic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PresenceButton {
    pub label: String,
    pub url: String,
}

/// What should be published right now, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Decision {
    /// The feature is switched off.
    Off,
    /// Nothing is being watched or read, or it is paused.
    Idle,
    /// Blacklisted by the user: not even a generic presence.
    Blocked { label: String },
    Show { presence: Presence, reason: Reason },
}

/// Why the presence looks the way it does. Shown in the settings preview so the
/// stripping is visible rather than mysterious.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reason {
    Full,
    /// The work is flagged `isAdult` on NihongoTracker.
    Adult,
    /// The media could not be resolved, so the title is just a file name.
    Unresolved,
    /// The user asked for activity only, with no titles anywhere.
    Generic,
}

// ── deciding what to publish ───────────────────────────────────────────

fn epoch_of(iso: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp())
}

fn blacklisted<'a>(list: &'a [BlacklistEntry], key: &str) -> Option<&'a BlacklistEntry> {
    let needle = key.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    list.iter().find(|entry| entry.key == needle)
}

/// The whole policy, as a pure function: same inputs, same answer, whether it
/// is the watcher publishing or the settings screen previewing.
pub fn decide(
    snapshot: &TrackerSnapshot,
    settings: &Settings,
    blacklist: &[BlacklistEntry],
) -> Decision {
    if !settings.discord_rpc {
        return Decision::Off;
    }
    let lang = settings.language.as_str();

    let button = match (settings.discord_profile_button, settings.username.as_deref()) {
        (true, Some(username)) if !username.trim().is_empty() => Some(PresenceButton {
            label: crate::i18n::discord_profile(lang).to_string(),
            url: format!(
                "{}/user/{}",
                settings.api_base_url.trim_end_matches('/'),
                username.trim()
            ),
        }),
        _ => None,
    };

    // A reading session wins over a video: if both are somehow live, the one
    // the user is actually interacting with is the novel in the foreground.
    if let Some(vn) = snapshot.vn.as_ref().filter(|vn| vn.counting) {
        if let Some(entry) = blacklisted(blacklist, &vn.content_id).or_else(|| blacklisted(blacklist, &vn.exe)) {
            return Decision::Blocked {
                label: entry.label.clone(),
            };
        }
        let reason = if vn.is_adult {
            Reason::Adult
        } else if settings.discord_generic_mode {
            Reason::Generic
        } else {
            Reason::Full
        };
        let hide = reason != Reason::Full;
        return Decision::Show {
            presence: Presence {
                details: if hide {
                    crate::i18n::discord_generic(lang).to_string()
                } else {
                    crate::i18n::discord_reading(lang, &vn.media_title)
                },
                state: crate::i18n::discord_chars(lang, vn.session_chars),
                large_image: match (hide, vn.image.as_deref()) {
                    (false, Some(image)) if !image.is_empty() => image.to_string(),
                    _ => settings.discord_large_image.clone(),
                },
                large_text: if hide {
                    crate::i18n::discord_generic(lang).to_string()
                } else {
                    vn.media_title.clone()
                },
                start_epoch: epoch_of(&vn.started_at),
                button,
                generic: hide,
            },
            reason,
        };
    }

    if let Some(anime) = snapshot.anime.as_ref().filter(|anime| !anime.paused) {
        let media = anime.media.as_ref();
        let content_id = media.map(|m| m.content_id.as_str()).unwrap_or_default();
        if let Some(entry) = blacklisted(blacklist, content_id)
            .or_else(|| blacklisted(blacklist, &anime.player))
        {
            return Decision::Blocked {
                label: entry.label.clone(),
            };
        }
        // An unresolved episode only has the release file name to go on, which
        // is exactly the kind of string nobody wants broadcast.
        let reason = match media {
            Some(m) if m.is_adult => Reason::Adult,
            None => Reason::Unresolved,
            Some(_) if settings.discord_generic_mode => Reason::Generic,
            Some(_) => Reason::Full,
        };
        let hide = reason != Reason::Full;
        let title = media.map(|m| m.title.clone()).unwrap_or_default();
        return Decision::Show {
            presence: Presence {
                details: if hide {
                    crate::i18n::discord_generic(lang).to_string()
                } else {
                    crate::i18n::discord_watching(lang, &title)
                },
                state: crate::i18n::discord_episode_state(lang, anime.parsed.episode),
                large_image: match (hide, media.and_then(|m| m.image.as_deref())) {
                    (false, Some(image)) if !image.is_empty() => image.to_string(),
                    _ => settings.discord_large_image.clone(),
                },
                large_text: if hide {
                    crate::i18n::discord_generic(lang).to_string()
                } else {
                    title
                },
                start_epoch: epoch_of(&anime.started_at),
                button,
                generic: hide,
            },
            reason,
        };
    }

    Decision::Idle
}

// ── the connection thread ──────────────────────────────────────────────

pub enum Command {
    /// Publish this presence, subject to the rate limit.
    Set(Box<Presence>),
    /// Wipe the presence. Never throttled: a stale presence is a privacy leak.
    Clear,
    /// The application id changed (or the feature was toggled).
    Reconnect(String),
    Shutdown,
}

pub struct Discord {
    tx: Sender<Command>,
    pub connected: Arc<AtomicBool>,
}

impl Discord {
    pub fn send(&self, command: Command) {
        // A dead thread must never take the caller down with it.
        let _ = self.tx.send(command);
    }
}

pub fn spawn(app: AppHandle, app_id: String) -> Discord {
    let (tx, rx) = std::sync::mpsc::channel::<Command>();
    let connected = Arc::new(AtomicBool::new(false));
    let flag = connected.clone();

    // A plain OS thread: the IPC in `discord-rich-presence` is blocking, and it
    // has no business sitting on a tokio worker.
    std::thread::Builder::new()
        .name("discord-rpc".into())
        .spawn(move || run(app, rx, flag, app_id))
        .expect("spawn discord thread");

    Discord { tx, connected }
}

struct Connection {
    client: Option<discord_rich_presence::DiscordIpcClient>,
    app_id: String,
    /// Never before this instant; moves out on every failed attempt.
    next_attempt: Instant,
    backoff: Duration,
}

impl Connection {
    fn new(app_id: String) -> Self {
        Self {
            client: None,
            app_id,
            next_attempt: Instant::now(),
            backoff: BACKOFF_START,
        }
    }

    fn drop_client(&mut self) {
        use discord_rich_presence::DiscordIpc;
        if let Some(client) = self.client.as_mut() {
            let _ = client.close();
        }
        self.client = None;
        self.next_attempt = Instant::now() + self.backoff;
        self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
    }

    /// `true` once there is a live client. Never blocks for long: a missing
    /// Discord fails immediately and is retried later.
    fn ensure(&mut self) -> bool {
        use discord_rich_presence::DiscordIpc;
        if self.client.is_some() {
            return true;
        }
        if self.app_id.trim().is_empty() || Instant::now() < self.next_attempt {
            return false;
        }
        let mut client = discord_rich_presence::DiscordIpcClient::new(self.app_id.trim());
        match client.connect() {
            Ok(()) => {
                self.client = Some(client);
                self.backoff = BACKOFF_START;
                true
            }
            Err(_) => {
                // Discord simply isn't running yet. Not an error worth shouting
                // about; try again a little later.
                self.next_attempt = Instant::now() + self.backoff;
                self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
                false
            }
        }
    }
}

fn run(app: AppHandle, rx: Receiver<Command>, connected: Arc<AtomicBool>, app_id: String) {
    let mut conn = Connection::new(app_id);
    // `Some(None)` means "a clear is pending", `Some(Some(p))` a presence.
    let mut pending: Option<Option<Presence>> = None;
    let mut published: Option<Presence> = None;
    let mut last_send = Instant::now() - MIN_INTERVAL;

    loop {
        match rx.recv_timeout(TICK) {
            Ok(Command::Set(presence)) => {
                // Replaces whatever was waiting: intermediate states are
                // dropped, not queued.
                pending = Some(Some(*presence));
            }
            Ok(Command::Clear) => pending = Some(None),
            Ok(Command::Reconnect(id)) => {
                if let Some(client) = conn.client.as_mut() {
                    use discord_rich_presence::DiscordIpc;
                    let _ = client.clear_activity();
                }
                conn.client = None;
                conn.app_id = id;
                conn.backoff = BACKOFF_START;
                conn.next_attempt = Instant::now();
                published = None;
            }
            Ok(Command::Shutdown) => {
                use discord_rich_presence::DiscordIpc;
                if let Some(client) = conn.client.as_mut() {
                    let _ = client.clear_activity();
                    let _ = client.close();
                }
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The app is going away and took the sender with it.
            Err(RecvTimeoutError::Disconnected) => {
                use discord_rich_presence::DiscordIpc;
                if let Some(client) = conn.client.as_mut() {
                    let _ = client.clear_activity();
                    let _ = client.close();
                }
                return;
            }
        }

        let Some(target) = pending.clone() else {
            report(&app, &connected, conn.client.is_some());
            continue;
        };

        // Nothing changed since the last successful publish.
        if target == published {
            pending = None;
            report(&app, &connected, conn.client.is_some());
            continue;
        }

        // Clearing is a privacy action, so it skips the rate limit; publishing
        // waits for the window.
        let clearing = target.is_none();
        if !clearing && last_send.elapsed() < MIN_INTERVAL {
            report(&app, &connected, conn.client.is_some());
            continue;
        }

        if !conn.ensure() {
            report(&app, &connected, false);
            continue;
        }

        let outcome = {
            use discord_rich_presence::DiscordIpc;
            let client = conn.client.as_mut().expect("ensured above");
            match &target {
                None => client.clear_activity(),
                Some(presence) => client.set_activity(to_activity(presence)),
            }
        };

        match outcome {
            Ok(()) => {
                published = target;
                pending = None;
                last_send = Instant::now();
                report(&app, &connected, true);
            }
            Err(_) => {
                // Discord went away mid-session: keep `pending` so the state is
                // republished once it comes back.
                conn.drop_client();
                published = None;
                report(&app, &connected, false);
            }
        }
    }
}

fn report(app: &AppHandle, connected: &Arc<AtomicBool>, now: bool) {
    if connected.swap(now, Ordering::AcqRel) != now {
        let _ = app.emit(EVT_STATUS, serde_json::json!({ "connected": now }));
    }
}

fn to_activity(presence: &Presence) -> discord_rich_presence::activity::Activity<'_> {
    use discord_rich_presence::activity::{Activity, Assets, Button, Timestamps};

    let mut activity = Activity::new()
        .details(presence.details.as_str())
        .state(presence.state.as_str())
        .timestamps(Timestamps::new().start(presence.start_epoch));

    let mut assets = Assets::new();
    if !presence.large_image.is_empty() {
        assets = assets.large_image(presence.large_image.as_str());
    }
    if !presence.large_text.is_empty() {
        assets = assets.large_text(presence.large_text.as_str());
    }
    activity = activity.assets(assets);

    if let Some(button) = presence.button.as_ref() {
        activity = activity.buttons(vec![Button::new(
            button.label.as_str(),
            button.url.as_str(),
        )]);
    }
    activity
}

// ── what the watcher calls ─────────────────────────────────────────────

/// Publish (or withdraw) the presence for the current tick. Cheap: it only
/// hands a message to the thread, which owns the rate limit and the socket.
pub async fn update(app: &AppHandle, snapshot: &TrackerSnapshot, settings: &Settings) {
    let state = app.state::<AppState>();
    let Some(discord) = state.discord.read().await.as_ref().map(|d| d.tx.clone()) else {
        return;
    };
    let blacklist = if settings.discord_rpc {
        state.db.discord_blacklist().unwrap_or_default()
    } else {
        Vec::new()
    };
    let command = match decide(snapshot, settings, &blacklist) {
        Decision::Show { presence, .. } => Command::Set(Box::new(presence)),
        _ => Command::Clear,
    };
    let _ = discord.send(command);
}

/// Start, restart or stop the connection thread after a settings change.
pub async fn apply_settings(app: &AppHandle, settings: &Settings) {
    let state = app.state::<AppState>();
    let mut slot = state.discord.write().await;

    if !settings.discord_rpc {
        if let Some(discord) = slot.take() {
            // Leaving last night's reading on someone's profile is exactly the
            // failure this feature must not have.
            discord.send(Command::Clear);
            discord.send(Command::Shutdown);
        }
        let _ = app.emit(EVT_STATUS, serde_json::json!({ "connected": false }));
        return;
    }

    match slot.as_ref() {
        Some(discord) => discord.send(Command::Reconnect(settings.discord_app_id.clone())),
        None => *slot = Some(spawn(app.clone(), settings.discord_app_id.clone())),
    }
}

/// Wipe the presence on the way out. Called from the exit handler, where there
/// is no async runtime left to await on.
pub fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(slot) = state.discord.try_read() else {
        return;
    };
    if let Some(discord) = slot.as_ref() {
        discord.send(Command::Clear);
        discord.send(Command::Shutdown);
        // The write is one small blocking call on the other thread; give it a
        // moment to land before the process disappears.
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParsedAnime;
    use crate::playback::PlaybackSource;
    use crate::state::{AnimeSession, MediaMatch, VnSession};

    fn settings() -> Settings {
        Settings {
            discord_rpc: true,
            discord_app_id: "123".into(),
            username: Some("hitogi".into()),
            api_base_url: "https://nihongotracker.app".into(),
            ..Default::default()
        }
    }

    fn vn(is_adult: bool) -> VnSession {
        VnSession {
            exe: "aokana.exe".into(),
            content_id: "v18".into(),
            media_title: "蒼の彼方のフォーリズム".into(),
            media_type: "vn".into(),
            image: Some("https://img/cover.jpg".into()),
            is_adult,
            elapsed_secs: 600,
            counting: true,
            started_at: "2026-08-12T20:00:00Z".into(),
            session_chars: 12_400,
            session_lines: 300,
        }
    }

    fn anime(media: Option<MediaMatch>) -> AnimeSession {
        AnimeSession {
            key: "k".into(),
            player: "mpv".into(),
            file_name: "[Group] Show - 03 [1080p].mkv".into(),
            parsed: ParsedAnime {
                title: "Show".into(),
                episode: Some(3),
                episode_end: None,
                season: None,
                group: None,
                file_name: "[Group] Show - 03 [1080p].mkv".into(),
            },
            media,
            resolved: true,
            unmatched: false,
            elapsed_secs: 300,
            threshold_secs: 900,
            threshold_percent: 70,
            position_secs: None,
            duration_secs: None,
            paused: false,
            source: PlaybackSource::Elapsed,
            progress: 0.3,
            logged: false,
            started_at: "2026-08-12T20:00:00Z".into(),
        }
    }

    fn media(is_adult: bool) -> MediaMatch {
        MediaMatch {
            content_id: "21".into(),
            title: "One Piece".into(),
            media_type: "anime".into(),
            image: Some("https://img/op.jpg".into()),
            episode_duration: Some(24),
            is_adult,
            manual: false,
        }
    }

    fn snapshot_vn(session: VnSession) -> TrackerSnapshot {
        TrackerSnapshot {
            vn: Some(session),
            ..Default::default()
        }
    }

    fn snapshot_anime(session: AnimeSession) -> TrackerSnapshot {
        TrackerSnapshot {
            anime: Some(session),
            ..Default::default()
        }
    }

    fn entry(key: &str, kind: &str) -> BlacklistEntry {
        BlacklistEntry {
            key: key.into(),
            kind: kind.into(),
            label: "hidden".into(),
        }
    }

    #[test]
    fn off_by_default() {
        let mut s = settings();
        s.discord_rpc = false;
        assert_eq!(decide(&snapshot_vn(vn(false)), &s, &[]), Decision::Off);
        // And the default settings really do have it off.
        assert!(!Settings::default().discord_rpc);
    }

    #[test]
    fn a_reading_session_publishes_title_and_progress() {
        let Decision::Show { presence, reason } = decide(&snapshot_vn(vn(false)), &settings(), &[])
        else {
            panic!("expected a presence");
        };
        assert_eq!(reason, Reason::Full);
        assert!(presence.details.contains("蒼の彼方のフォーリズム"));
        assert_eq!(presence.state, "12.4k characters");
        assert_eq!(presence.large_image, "https://img/cover.jpg");
        // The session start, so Discord runs the clock itself.
        assert_eq!(presence.start_epoch, 1786564800);
        assert_eq!(
            presence.button.unwrap().url,
            "https://nihongotracker.app/user/hitogi"
        );
        assert!(!presence.generic);
    }

    #[test]
    fn adult_media_never_leaks_its_title_or_cover() {
        let Decision::Show { presence, reason } = decide(&snapshot_vn(vn(true)), &settings(), &[])
        else {
            panic!("expected a presence");
        };
        assert_eq!(reason, Reason::Adult);
        assert!(presence.generic);
        assert!(!presence.details.contains("蒼"));
        assert!(!presence.large_text.contains("蒼"));
        assert_ne!(presence.large_image, "https://img/cover.jpg");
        // The progress is not identifying, so it stays.
        assert_eq!(presence.state, "12.4k characters");
    }

    #[test]
    fn an_unresolved_episode_falls_back_to_generic() {
        let Decision::Show { presence, reason } =
            decide(&snapshot_anime(anime(None)), &settings(), &[])
        else {
            panic!("expected a presence");
        };
        assert_eq!(reason, Reason::Unresolved);
        // The release file name is the last thing anyone wants broadcast.
        assert!(!presence.details.contains("Group"));
        assert!(!presence.large_text.contains("Group"));
        assert_eq!(presence.state, "Episode 3");
    }

    #[test]
    fn adult_anime_is_stripped_too() {
        let Decision::Show { reason, presence } =
            decide(&snapshot_anime(anime(Some(media(true)))), &settings(), &[])
        else {
            panic!("expected a presence");
        };
        assert_eq!(reason, Reason::Adult);
        assert!(!presence.details.contains("One Piece"));
    }

    #[test]
    fn generic_mode_hides_every_title() {
        let mut s = settings();
        s.discord_generic_mode = true;
        let Decision::Show { presence, reason } =
            decide(&snapshot_anime(anime(Some(media(false)))), &s, &[])
        else {
            panic!("expected a presence");
        };
        assert_eq!(reason, Reason::Generic);
        assert_eq!(presence.details, "Japanese immersion");
        assert!(!presence.details.contains("One Piece"));
    }

    #[test]
    fn the_blacklist_publishes_nothing_at_all() {
        let by_media = decide(&snapshot_vn(vn(false)), &settings(), &[entry("v18", "media")]);
        assert!(matches!(by_media, Decision::Blocked { .. }));

        let by_process = decide(
            &snapshot_vn(vn(false)),
            &settings(),
            &[entry("aokana.exe", "process")],
        );
        assert!(matches!(by_process, Decision::Blocked { .. }));

        let unrelated = decide(&snapshot_vn(vn(false)), &settings(), &[entry("v99", "media")]);
        assert!(matches!(unrelated, Decision::Show { .. }));
    }

    #[test]
    fn a_paused_session_shows_nothing() {
        let mut paused = vn(false);
        paused.counting = false;
        assert_eq!(decide(&snapshot_vn(paused), &settings(), &[]), Decision::Idle);

        let mut stopped = anime(Some(media(false)));
        stopped.paused = true;
        assert_eq!(
            decide(&snapshot_anime(stopped), &settings(), &[]),
            Decision::Idle
        );
    }

    #[test]
    fn the_profile_button_can_be_turned_off() {
        let mut s = settings();
        s.discord_profile_button = false;
        let Decision::Show { presence, .. } = decide(&snapshot_vn(vn(false)), &s, &[]) else {
            panic!("expected a presence");
        };
        assert!(presence.button.is_none());

        // And it is absent anyway when there is no username to point at.
        let mut anonymous = settings();
        anonymous.username = None;
        let Decision::Show { presence, .. } = decide(&snapshot_vn(vn(false)), &anonymous, &[])
        else {
            panic!("expected a presence");
        };
        assert!(presence.button.is_none());
    }
}
