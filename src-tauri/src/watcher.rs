//! The detection loop: reads the window list once a second, keeps an anime and
//! a visual-novel session in sync with what is actually on screen, and turns a
//! finished session into a log (queued or parked for confirmation).

use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Settings;
use crate::db::TextState;
use crate::parser;
use crate::platform;
use crate::players;
use crate::state::{AnimeSession, AppState, MediaMatch, TrackerSnapshot, VnSession};
use crate::sync;

pub const EVT_TRACKER: &str = "tracker://state";
pub const EVT_VN_STARTED: &str = "vn://started";
pub const EVT_VN_ENDED: &str = "vn://ended";

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut sys = System::new();
        let mut persist_countdown: u64 = 0;
        loop {
            let settings = {
                let state = app.state::<AppState>();
                state.settings_snapshot().await
            };
            let interval = settings.poll_interval_ms.clamp(250, 10_000);
            let tick_secs = (interval as f64 / 1000.0).max(0.25);

            tick(&app, &mut sys, &settings, tick_secs, &mut persist_countdown).await;
            tokio::time::sleep(Duration::from_millis(interval)).await;
        }
    });
}

struct Observed {
    windows: Vec<platform::WindowInfo>,
    foreground_process: Option<String>,
    foreground_title: Option<String>,
    idle_seconds: u64,
    running: HashMap<String, u32>,
}

fn observe(sys: &mut System) -> Observed {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::new());

    let mut running: HashMap<String, u32> = HashMap::new();
    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_ascii_lowercase();
        running.entry(name).or_insert(pid.as_u32());
    }

    let name_of = |pid: u32| -> String {
        sys.process(Pid::from_u32(pid))
            .map(|p| p.name().to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    };

    let windows = platform::visible_windows()
        .into_iter()
        .map(|mut w| {
            w.process = name_of(w.pid);
            w
        })
        .collect();

    let fg = platform::foreground_window();
    Observed {
        windows,
        foreground_process: fg.as_ref().map(|w| name_of(w.pid)),
        foreground_title: fg.map(|w| w.title),
        idle_seconds: platform::idle_seconds(),
        running,
    }
}

async fn tick(
    app: &AppHandle,
    sys: &mut System,
    settings: &Settings,
    tick_secs: f64,
    persist_countdown: &mut u64,
) {
    let observed = observe(sys);
    let state = app.state::<AppState>();

    let mut snapshot = state.tracker.read().await.clone();
    snapshot.platform_supported = platform::supported();
    snapshot.enabled = settings.detect_players || settings.detect_visual_novels;
    snapshot.active_process = observed.foreground_process.clone();
    snapshot.active_window = observed.foreground_title.clone();
    snapshot.idle_seconds = observed.idle_seconds;

    if settings.detect_players {
        update_anime(app, &mut snapshot, &observed, settings, tick_secs).await;
    } else {
        snapshot.anime = None;
    }

    if settings.detect_visual_novels {
        update_vn(app, &mut snapshot, &observed, settings, tick_secs, persist_countdown).await;
    } else if snapshot.vn.is_some() {
        finish_vn(app, &mut snapshot, settings).await;
    }

    snapshot.queued_count = state.db.queued_count();
    snapshot.pending_count = state.db.list_pending().map(|p| p.len() as i64).unwrap_or(0);

    *state.tracker.write().await = snapshot.clone();
    let _ = app.emit(EVT_TRACKER, &snapshot);
}

// ── anime ──────────────────────────────────────────────────────────────

async fn update_anime(
    app: &AppHandle,
    snapshot: &mut TrackerSnapshot,
    observed: &Observed,
    settings: &Settings,
    tick_secs: f64,
) {
    let detected = observed.windows.iter().find_map(|w| {
        let player = players::lookup(&w.process)?;
        let file = player.extract_file_name(&w.title)?;
        Some((player.name.to_string(), file))
    });

    let Some((player_name, file_name)) = detected else {
        // Player closed or went idle: drop the session (an unfinished episode
        // is simply not logged).
        snapshot.anime = None;
        return;
    };

    let parsed = parser::parse(&file_name);
    let key = format!(
        "{}|s{}|e{}",
        parsed.title.to_lowercase(),
        parsed.season.unwrap_or(1),
        parsed.episode.map(|e| e.to_string()).unwrap_or_else(|| "?".into())
    );

    let is_new = snapshot
        .anime
        .as_ref()
        .map(|s| s.key != key)
        .unwrap_or(true);

    if is_new {
        let title = parsed.title.clone();
        snapshot.anime = Some(AnimeSession {
            key: key.clone(),
            player: player_name,
            file_name: file_name.clone(),
            resolved: false,
            unmatched: false,
            parsed,
            media: None,
            elapsed_secs: 0,
            threshold_secs: threshold_for(settings, None),
            logged: false,
            started_at: crate::db::now_iso(),
        });
        // The lookup runs off the tick so a slow network never stalls detection.
        request_media(app, &title, "anime");
    }

    let Some(session) = snapshot.anime.as_mut() else {
        return;
    };

    if !session.resolved {
        if let Some(result) = peek_media(app, &session.parsed.title, "anime").await {
            session.threshold_secs = threshold_for(settings, result.as_ref());
            session.unmatched = result.is_none();
            session.media = result;
            session.resolved = true;
        }
    }

    session.elapsed_secs += tick_secs.round() as u64;

    if session.ready() && !session.logged {
        session.logged = true;
        let session = session.clone();
        commit_anime(app, &session, settings, false).await;
    }
}

fn threshold_for(settings: &Settings, media: Option<&MediaMatch>) -> u64 {
    let minutes = media
        .and_then(|m| m.episode_duration)
        .filter(|d| *d > 0)
        .unwrap_or(settings.anime_episode_minutes);
    let secs = (minutes as u64 * 60)
        .saturating_mul(settings.anime_threshold_percent.clamp(1, 100) as u64)
        / 100;
    secs.max(30)
}

/// Turn a finished (or manually accepted) episode into a log.
/// `forced` comes from the user pressing "log now", which also means the
/// confirmation step is redundant. Returns true when the log was queued for
/// delivery rather than parked for confirmation.
pub async fn commit_anime(
    app: &AppHandle,
    session: &AnimeSession,
    settings: &Settings,
    forced: bool,
) -> bool {
    let state = app.state::<AppState>();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let dedup_key = format!("anime:{}:{today}", session.key);
    if state.db.was_logged(&dedup_key) {
        return false;
    }

    let title = session
        .media
        .as_ref()
        .map(|m| m.title.clone())
        .unwrap_or_else(|| session.parsed.title.clone());
    if title.trim().is_empty() {
        return false;
    }

    let mut payload = json!({
        "type": "anime",
        "description": title,
        "episodes": session.parsed.episode_count(),
        "date": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(media) = &session.media {
        payload["mediaId"] = json!(media.content_id);
    }
    if settings.anime_log_time {
        let minutes = (session.elapsed_secs / 60).max(1);
        payload["time"] = json!(minutes);
    }
    // Extra context the UI shows while the log waits for confirmation.
    payload["_detected"] = json!({
        "episode": session.parsed.episode,
        "season": session.parsed.season,
        "file": session.file_name,
        "player": session.player,
        "image": session.media.as_ref().and_then(|m| m.image.clone()),
    });

    let _ = state.db.remember_logged(&dedup_key);

    let episode_label = session
        .parsed
        .episode
        .map(|e| format!("ep. {e}"))
        .unwrap_or_else(|| "episodio".into());

    // Pressing "log now" is itself the confirmation, so it skips the queue.
    let send_now = forced || (settings.anime_auto_log && session.media.is_some());

    if send_now {
        sync::enqueue_log(app, &payload);
        notify(
            app,
            settings,
            "Episodio registrado",
            &format!("{title} — {episode_label}"),
        );
        flush_later(app);
    } else {
        sync::add_pending(app, &payload, "anime");
        notify(
            app,
            settings,
            "Listo para registrar",
            &format!("{title} — {episode_label}"),
        );
    }
    send_now
}

// ── visual novels ──────────────────────────────────────────────────────

async fn update_vn(
    app: &AppHandle,
    snapshot: &mut TrackerSnapshot,
    observed: &Observed,
    settings: &Settings,
    tick_secs: f64,
    persist_countdown: &mut u64,
) {
    let state = app.state::<AppState>();
    let apps = state.db.list_vn_apps().unwrap_or_default();

    let running = apps
        .iter()
        .filter(|a| a.enabled)
        .find(|a| observed.running.contains_key(&a.exe.to_ascii_lowercase()));

    match (running, snapshot.vn.is_some()) {
        (None, true) => {
            finish_vn(app, snapshot, settings).await;
            return;
        }
        (None, false) => return,
        (Some(_), _) => {}
    }

    let vn_app = running.expect("checked above");
    let restart = snapshot
        .vn
        .as_ref()
        .map(|s| s.exe != vn_app.exe)
        .unwrap_or(true);

    if restart {
        if snapshot.vn.is_some() {
            finish_vn(app, snapshot, settings).await;
        }
        let stored = state.db.get_text_state(&vn_app.media_id);
        let (lines, chars) = state.db.text_totals(&vn_app.media_id);
        let _ = state.db.save_text_state(&TextState {
            content_id: vn_app.media_id.clone(),
            timer_seconds: stored.timer_seconds,
            baseline_chars: chars,
            baseline_lines: lines,
            media_type: Some(vn_app.media_type.clone()),
            media_title: Some(vn_app.media_title.clone()),
        });

        snapshot.vn = Some(VnSession {
            exe: vn_app.exe.clone(),
            content_id: vn_app.media_id.clone(),
            media_title: vn_app.media_title.clone(),
            media_type: vn_app.media_type.clone(),
            image: vn_app.image.clone(),
            elapsed_secs: 0,
            counting: false,
            started_at: crate::db::now_iso(),
            session_chars: 0,
            session_lines: 0,
        });

        // Point the hooker at this work from here, not from the frontend: the
        // window may be hidden in the tray and its listener never fire.
        {
            let mut hooker = state.hooker.write().await;
            hooker.content_id = Some(vn_app.media_id.clone());
            hooker.content_title = Some(vn_app.media_title.clone());
        }
        let hooker_idle = {
            let hooker = state.hooker.read().await;
            hooker.status != crate::state::HookerStatus::Connected
        };
        if settings.texthooker_autoconnect && hooker_idle {
            let _ = state
                .hooker_tx
                .send(crate::state::HookerCommand::Connect(
                    settings.texthooker_ws_url.clone(),
                ));
        }
        let hooker_snapshot = state.hooker.read().await.clone();
        let _ = app.emit(crate::texthooker::EVT_STATUS, &hooker_snapshot);

        let _ = app.emit(
            EVT_VN_STARTED,
            json!({
                "contentId": vn_app.media_id,
                "title": vn_app.media_title,
                "type": vn_app.media_type,
                "image": vn_app.image,
                "openTexthooker": settings.vn_open_texthooker,
                "externalUrl": vn_app.texthooker_url,
            }),
        );
        notify(
            app,
            settings,
            "Sesión iniciada",
            &format!("{} detectada", vn_app.media_title),
        );
    }

    let focused = observed
        .foreground_process
        .as_deref()
        .map(|p| p == vn_app.exe.to_ascii_lowercase())
        .unwrap_or(false);
    let afk = settings.afk_timeout_secs > 0 && observed.idle_seconds >= settings.afk_timeout_secs;
    let counting = (!settings.vn_require_focus || focused) && !afk;

    let content_id = vn_app.media_id.clone();
    let (lines, chars) = state.db.text_totals(&content_id);
    let baseline = state.db.get_text_state(&content_id);

    if let Some(session) = snapshot.vn.as_mut() {
        session.counting = counting;
        if counting {
            session.elapsed_secs += tick_secs.round() as u64;
        }
        session.session_chars = (chars - baseline.baseline_chars).max(0);
        session.session_lines = (lines - baseline.baseline_lines).max(0);
    }

    // Keep the on-disk timer close to reality without hammering SQLite.
    *persist_countdown += 1;
    if *persist_countdown >= 15 {
        *persist_countdown = 0;
        if let Some(session) = snapshot.vn.as_ref() {
            let _ = state.db.save_text_state(&TextState {
                content_id: content_id.clone(),
                timer_seconds: baseline.timer_seconds.max(session.elapsed_secs as i64),
                baseline_chars: baseline.baseline_chars,
                baseline_lines: baseline.baseline_lines,
                media_type: Some(session.media_type.clone()),
                media_title: Some(session.media_title.clone()),
            });
        }
    }
}

/// Close the current VN session and turn it into a log.
async fn finish_vn(app: &AppHandle, snapshot: &mut TrackerSnapshot, settings: &Settings) {
    let Some(session) = snapshot.vn.take() else {
        return;
    };
    let state = app.state::<AppState>();
    let minutes = session.elapsed_secs / 60;

    let _ = app.emit(
        EVT_VN_ENDED,
        json!({
            "contentId": session.content_id,
            "title": session.media_title,
            "minutes": minutes,
            "chars": session.session_chars,
        }),
    );

    if minutes == 0 && session.session_chars == 0 {
        return;
    }

    let mut payload = json!({
        "type": session.media_type,
        "mediaId": session.content_id,
        "description": session.media_title,
        "date": chrono::Utc::now().to_rfc3339(),
    });
    if minutes > 0 {
        payload["time"] = json!(minutes);
    }
    if session.session_chars > 0 {
        payload["chars"] = json!(session.session_chars);
    }
    payload["_detected"] = json!({
        "source": "vn",
        "exe": session.exe,
        "lines": session.session_lines,
        "seconds": session.elapsed_secs,
        "image": session.image,
    });

    if settings.vn_confirm_before_log {
        sync::add_pending(app, &payload, "vn");
        notify(
            app,
            settings,
            "Sesión terminada",
            &format!(
                "{} — {} min, {} car.",
                session.media_title, minutes, session.session_chars
            ),
        );
    } else {
        sync::enqueue_log(app, &payload);
        queue_history(app, &session.content_id, &session, minutes);
        notify(
            app,
            settings,
            "Sesión registrada",
            &format!(
                "{} — {} min, {} car.",
                session.media_title, minutes, session.session_chars
            ),
        );
        flush_later(app);
    }

    // The session baseline moves forward so the next run starts from zero.
    let (lines, chars) = state.db.text_totals(&session.content_id);
    let _ = state.db.save_text_state(&TextState {
        content_id: session.content_id.clone(),
        timer_seconds: 0,
        baseline_chars: chars,
        baseline_lines: lines,
        media_type: Some(session.media_type.clone()),
        media_title: Some(session.media_title.clone()),
    });
}

pub fn queue_history(app: &AppHandle, content_id: &str, session: &VnSession, minutes: u64) {
    let state = app.state::<AppState>();
    let speed = if session.elapsed_secs > 0 {
        (session.session_chars as f64 * 3600.0 / session.elapsed_secs as f64).round() as i64
    } else {
        0
    };
    let payload = json!({
        "contentId": content_id,
        "entry": {
            "isShared": false,
            "connectedUsersCount": 0,
            "linesLogged": session.session_lines,
            "charactersLogged": session.session_chars,
            "readingSpeed": speed,
            "sessionSeconds": session.elapsed_secs,
            "loggedAt": chrono::Utc::now().to_rfc3339(),
        },
        "minutes": minutes,
    });
    let _ = state
        .db
        .enqueue(&sync::new_id(), "texthooker_history", &payload.to_string());
}

// ── helpers ────────────────────────────────────────────────────────────

/// Push the outbox without blocking the detection tick.
fn flush_later(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        sync::flush(&app).await;
    });
}

fn cache_key(title: &str, media_type: &str) -> String {
    format!("{media_type}:{}", title.to_lowercase())
}

/// Read the resolution cache. `None` means "still unknown"; `Some(None)` means
/// the search finished and found nothing.
pub async fn peek_media(
    app: &AppHandle,
    title: &str,
    media_type: &str,
) -> Option<Option<MediaMatch>> {
    let state = app.state::<AppState>();
    let cache = state.media_cache.read().await;
    let found = cache.get(&cache_key(title, media_type)).cloned();
    found
}

/// Resolve a detected title in the background: manual override first, then the
/// search API. The result lands in `media_cache` for the next tick to pick up.
pub fn request_media(app: &AppHandle, title: &str, media_type: &str) {
    let title = title.trim().to_string();
    if title.is_empty() {
        return;
    }
    let media_type = media_type.to_string();
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let key = cache_key(&title, &media_type);
        if state.media_cache.read().await.contains_key(&key) {
            return;
        }

        // A manual mapping wins over search.
        if let Some(raw) = state.db.get_kv(&format!("title_map:{}", title.to_lowercase())) {
            if let Ok(m) = serde_json::from_str::<MediaMatch>(&raw) {
                state.media_cache.write().await.insert(key, Some(m));
                return;
            }
        }

        let client = state.client().await;
        let found = client
            .search_media(&title, crate::api::search_type_for(&media_type), 5)
            .await;

        match found {
            Ok(hits) => {
                let matched = hits.into_iter().next().map(MediaMatch::from);
                state.media_cache.write().await.insert(key, matched);
            }
            // Network failure: leave it unknown so the next session retries.
            Err(_) => {}
        }
    });
}

fn notify(app: &AppHandle, settings: &Settings, title: &str, body: &str) {
    if !settings.notifications {
        return;
    }
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}
