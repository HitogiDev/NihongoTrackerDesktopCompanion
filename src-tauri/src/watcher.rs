//! The detection loop: reads the window list once a second, keeps an anime and
//! a visual-novel session in sync with what is actually on screen, and turns a
//! finished session into a log (queued or parked for confirmation).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::json;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Settings;
use crate::db::TextState;
use crate::parser;
use crate::platform;
use crate::players;
use crate::playback::{Playback, PlaybackSource};
use crate::state::{
    AnimeSession, AppState, MediaMatch, PlaybackSnapshot, TrackerSnapshot, VnSession,
};
use crate::sync;

pub const EVT_TRACKER: &str = "tracker://state";
pub const EVT_VN_STARTED: &str = "vn://started";
pub const EVT_VN_ENDED: &str = "vn://ended";

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut sys = System::new();
        let mut persist_countdown: u64 = 0;
        let mut last_tick = Instant::now();
        loop {
            let settings = {
                let state = app.state::<AppState>();
                state.settings_snapshot().await
            };
            let interval = settings.poll_interval_ms.clamp(250, 10_000);

            let now = Instant::now();
            let elapsed = credited_elapsed(now.duration_since(last_tick), interval);
            last_tick = now;

            tick(&app, &mut sys, &settings, elapsed, &mut persist_countdown).await;
            tokio::time::sleep(Duration::from_millis(interval)).await;
        }
    });
}

/// Real time between two ticks, which is what sessions are credited with: the
/// sleep comes after the tick, so the period is the interval plus however long
/// the tick took, and an interval like 750 ms is not a whole second. Capped so a
/// suspended machine or a stalled loop does not turn the gap into reading time.
fn credited_elapsed(since_last_tick: Duration, interval_ms: u64) -> Duration {
    let cap = Duration::from_millis(interval_ms.saturating_mul(3)).max(Duration::from_secs(5));
    since_last_tick.min(cap)
}

struct Observed {
    windows: Vec<platform::WindowInfo>,
    foreground_process: Option<String>,
    foreground_title: Option<String>,
    idle_seconds: u64,
    foreground_pid: Option<u32>,
    running: HashMap<String, Vec<ObservedProcess>>,
    /// Players on the desktop's media bus (MPRIS on Linux), for when their
    /// window titles cannot be read.
    media: Vec<platform::MediaPlayer>,
}

struct ObservedProcess {
    pid: u32,
    image_path: Option<PathBuf>,
}

impl Observed {
    fn active_window_summary(&self) -> (Option<String>, Option<String>) {
        if self.foreground_pid == Some(std::process::id()) {
            return (None, None);
        }
        (self.foreground_process.clone(), self.foreground_title.clone())
    }
}

fn observe(sys: &mut System) -> Observed {
    let refresh = ProcessRefreshKind::new().with_exe(UpdateKind::OnlyIfNotSet);
    // Outside Windows the command line names Wine / Proton games (see
    // `platform::process_key`).
    #[cfg(not(windows))]
    let refresh = refresh.with_cmd(UpdateKind::OnlyIfNotSet);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);

    let mut running: HashMap<String, Vec<ObservedProcess>> = HashMap::new();
    for (pid, process) in sys.processes() {
        running.entry(platform::process_key(process)).or_default().push(ObservedProcess {
            pid: pid.as_u32(),
            image_path: process.exe().map(PathBuf::from),
        });
    }

    let name_of = |pid: u32| -> String {
        sys.process(Pid::from_u32(pid))
            .map(platform::process_key)
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
        foreground_pid: fg.as_ref().map(|w| w.pid),
        foreground_title: fg.map(|w| w.title),
        idle_seconds: platform::idle_seconds(),
        running,
        media: platform::media_players(),
    }
}

async fn tick(
    app: &AppHandle,
    sys: &mut System,
    settings: &Settings,
    elapsed: Duration,
    persist_countdown: &mut u64,
) {
    let observed = observe(sys);
    let state = app.state::<AppState>();

    let mut snapshot = state.tracker.read().await.clone();
    apply_vn_timer_edit(&state, &mut snapshot);
    snapshot.capabilities = platform::capabilities();
    snapshot.platform_supported = snapshot.capabilities.window_titles
        || snapshot.capabilities.media_sessions
        || snapshot.capabilities.foreground;
    snapshot.enabled = settings.detect_players || settings.detect_visual_novels || settings.detect_games;
    (snapshot.active_process, snapshot.active_window) = observed.active_window_summary();
    snapshot.idle_seconds = observed.idle_seconds;

    if settings.detect_players {
        update_anime(app, &mut snapshot, &observed, settings, elapsed).await;
    } else {
        snapshot.anime = None;
    }

    if settings.detect_visual_novels || settings.detect_games {
        update_vn(app, &mut snapshot, &observed, settings, elapsed, persist_countdown).await;
    } else if snapshot.vn.is_some() {
        finish_vn(app, &mut snapshot, settings).await;
    }

    crate::reading_overlay::sync_session(app, &snapshot, settings);
    update_native_game_hook(app, &mut snapshot, &observed);
    crate::reading_overlay::sync_hook(app, &snapshot.game_hook);

    snapshot.queued_count = state.db.queued_count();
    snapshot.pending_count = state.db.list_pending().map(|p| p.len() as i64).unwrap_or(0);

    let mut current = state.tracker.write().await;
    apply_vn_timer_edit(&state, &mut snapshot);
    *current = snapshot.clone();
    drop(current);
    let _ = app.emit(EVT_TRACKER, &snapshot);
    crate::native_bridge::publish_status(app, &snapshot.game_hook).await;
    // Hands the presence to its own thread, which owns the rate limit and the
    // socket; nothing here can block on Discord.
    crate::discord::update(app, &snapshot, settings).await;
}

fn apply_vn_timer_edit(state: &AppState, snapshot: &mut TrackerSnapshot) {
    let Some(edit) = state.vn_timer_edit.lock().unwrap().take() else { return; };
    let Some(session) = snapshot.vn.as_mut() else { return; };
    if !edit.apply(session) { return; }
    let mut stored = state.db.get_text_state(&session.content_id);
    stored.timer_seconds = session.elapsed_secs as i64;
    let _ = state.db.save_text_state(&stored);
}

fn update_native_game_hook(app: &AppHandle, snapshot: &mut TrackerSnapshot, observed: &Observed) {
    let state = app.state::<AppState>();
    let target = snapshot.vn.as_ref().and_then(|session| {
        if session.media_type != "game" && session.media_type != "vn" {
            return None;
        }
        let entry = state
            .db
            .list_vn_apps()
            .ok()?
            .into_iter()
            .find(|entry| entry.enabled && entry.native_hook_enabled && entry.exe == session.exe)?;
        let processes = observed.running.get(&entry.exe)?;
        let process = processes
            .iter()
            .find(|process| process.pid == session.pid)
            .or_else(|| processes.first())?;
        Some(crate::native_hook::GameHookTarget {
            content_id: session.content_id.clone(),
            exe: entry.exe,
            pid: process.pid,
            image_path: process.image_path.clone(),
            selected_hook_code: entry.native_hook_code,
        })
    });
    state.game_hook.reconcile(app, target);
    snapshot.game_hook = state.game_hook.status();
}

// ── anime ──────────────────────────────────────────────────────────────

async fn update_anime(
    app: &AppHandle,
    snapshot: &mut TrackerSnapshot,
    observed: &Observed,
    settings: &Settings,
    elapsed: Duration,
) {
    // Window titles first, as on Windows; then the media bus, which is all a
    // Wayland session or a macOS install without Screen Recording offers.
    let detected = observed.windows.iter().find_map(|w| {
        let player = players::lookup(&w.process)?;
        let file = player.extract_file_name(&w.title)?;
        Some((player.name.to_string(), file, None))
    }).or_else(|| observed.media.iter().find_map(|media| {
        let player = players::lookup(&media.name)?;
        let file = players::media_file_name(media.url.as_deref(), media.title.as_deref())?;
        let file = player.extract_file_name(&file)?;
        Some((player.name.to_string(), file, Some(media)))
    }));

    let Some((player_name, file_name, media)) = detected else {
        // Player closed or went idle: drop the session (an unfinished episode
        // is simply not logged).
        snapshot.anime = None;
        let state = app.state::<AppState>();
        *state.playback.write().await = None;
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
            player: player_name.clone(),
            file_name: file_name.clone(),
            resolved: false,
            unmatched: false,
            parsed,
            media: None,
            elapsed_secs: 0,
            elapsed_carry_ms: 0,
            threshold_secs: threshold_for(settings, None),
            threshold_percent: settings.anime_threshold_percent.clamp(1, 100),
            position_secs: None,
            duration_secs: None,
            paused: false,
            source: PlaybackSource::Elapsed,
            progress: 0.0,
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

    // Where the player said it was on the previous tick. Probing runs off the
    // tick (see `request_playback`), so this is at most one second stale — which
    // does not matter over a 24-minute episode.
    let reported = match media {
        // The media bus already carries the position; no probe needed.
        Some(media) => media_playback(media).filter(|_| settings.use_player_position),
        None => {
            let reported = {
                let state = app.state::<AppState>();
                let cell = state.playback.read().await;
                cell.as_ref()
                    .filter(|snapshot| snapshot.player == player_name)
                    .map(|snapshot| snapshot.playback.clone())
            };
            request_playback(app, &player_name);
            reported
        }
    };

    match reported {
        Some(playback) => {
            session.position_secs = Some(playback.position_secs);
            session.duration_secs = Some(playback.duration_secs);
            session.paused = playback.paused;
            session.source = PlaybackSource::Player;
            if !playback.paused {
                session.credit_elapsed(elapsed);
            }
        }
        None => {
            session.position_secs = None;
            session.duration_secs = None;
            session.paused = false;
            session.source = PlaybackSource::Elapsed;
            session.credit_elapsed(elapsed);
        }
    }
    session.progress = session.compute_progress();

    if session.ready() && !session.logged {
        session.logged = true;
        let session = session.clone();
        commit_anime(app, &session, settings, false, "").await;
    }
}

fn media_playback(media: &platform::MediaPlayer) -> Option<Playback> {
    let duration = media.duration_secs.filter(|duration| *duration > 0.0)?;
    Some(Playback {
        position_secs: media.position_secs.unwrap_or(0.0),
        duration_secs: duration,
        paused: media.paused,
        file: media.url.clone(),
    })
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
    comment: &str,
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
        "mediaTitle": title,
        "episodes": session.parsed.episode_count(),
        "date": chrono::Utc::now().to_rfc3339(),
    });
    if forced && !comment.trim().is_empty() {
        payload["description"] = json!(comment.trim());
    }
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

    let lang = settings.language.as_str();
    let episode_label = crate::i18n::episode_label(lang, session.parsed.episode);

    // Pressing "log now" is itself the confirmation, so it skips the queue.
    let send_now = forced || (settings.anime_auto_log && session.media.is_some());

    if send_now {
        sync::enqueue_log(app, &payload);
        notify(
            app,
            settings,
            crate::i18n::episode_logged(lang),
            &format!("{title} — {episode_label}"),
        );
        flush_later(app);
    } else {
        sync::add_pending(app, &payload, "anime");
        notify(
            app,
            settings,
            crate::i18n::ready_to_log(lang),
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
    elapsed: Duration,
    persist_countdown: &mut u64,
) {
    let state = app.state::<AppState>();
    let apps = state.db.list_vn_apps().unwrap_or_default();

    let eligible = apps
        .iter()
        .filter(|a| a.enabled && if a.media_type == "game" { settings.detect_games } else { settings.detect_visual_novels })
        .filter(|a| observed.running.contains_key(&a.exe.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    // Prefer the foreground work. If focus moves elsewhere, retain the current
    // session rather than switching to another background process each tick.
    let running = eligible.iter().copied()
        .find(|a| observed.foreground_process.as_deref() == Some(a.exe.as_str()))
        .or_else(|| eligible.iter().copied().find(|a| snapshot.vn.as_ref().is_some_and(|s| s.exe == a.exe)))
        .or_else(|| eligible.first().copied());

    match (running, snapshot.vn.is_some()) {
        (None, true) => {
            finish_vn(app, snapshot, settings).await;
            return;
        }
        (None, false) => return,
        (Some(_), _) => {}
    }

    let vn_app = running.expect("checked above");
    let processes = observed
        .running
        .get(&vn_app.exe.to_ascii_lowercase())
        .expect("eligible VN app has a running process");
    let process = processes
        .iter()
        .find(|process| Some(process.pid) == observed.foreground_pid)
        .or_else(|| {
            snapshot
                .vn
                .as_ref()
                .filter(|session| session.exe == vn_app.exe)
                .and_then(|session| processes.iter().find(|process| process.pid == session.pid))
        })
        .or_else(|| processes.first())
        .expect("eligible VN app has a running process");
    let pid = process.pid;
    let restart = snapshot
        .vn
        .as_ref()
        .map(|s| s.exe != vn_app.exe || s.pid != pid)
        .unwrap_or(true);

    if restart {
        if snapshot.vn.is_some() {
            finish_vn(app, snapshot, settings).await;
        }
        state.session_paused.store(false, std::sync::atomic::Ordering::Release);
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
            pid,
            content_id: vn_app.media_id.clone(),
            media_title: vn_app.media_title.clone(),
            media_type: vn_app.media_type.clone(),
            image: vn_app.image.clone(),
            is_adult: vn_app.is_adult,
            is_adult_image: vn_app.is_adult_image,
            elapsed_secs: 0,
            elapsed_carry_ms: 0,
            counting: false,
            paused_by_user: false,
            started_at: crate::db::now_iso(),
            session_chars: 0,
            session_lines: 0,
        });

        // Keep the reader and mining context on the detected work, even if a
        // game does not open or connect the reader automatically.
        if vn_app.media_type == "vn" {
            crate::discord::request_vn_image_rating(app, vn_app.media_id.clone(), true);
        }
        {
            let mut hooker = state.hooker.write().await;
            hooker.content_id = Some(vn_app.media_id.clone());
            hooker.content_title = Some(vn_app.media_title.clone());
        }
        if settings.texthooker_autoconnect && !vn_app.native_hook_enabled {
            let _ = state.hooker_tx.send(crate::state::HookerCommand::Connect(settings.texthooker_ws_url.clone()));
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
            }),
        );
        // Opened from here, not from the main window's listener: that window
        // may not exist at all while the app lives in the tray.
        if settings.vn_open_texthooker {
            let app = app.clone();
            let content_id = vn_app.media_id.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::commands::open_work_texthooker(app, Some(content_id), true).await;
            });
        }
        notify(
            app,
            settings,
            crate::i18n::session_started(&settings.language),
            &crate::i18n::detected(&settings.language, &vn_app.media_title),
        );
    }

    // Where the focused window cannot be known (Wayland), focus is assumed:
    // otherwise a game, which only counts while focused, would never count.
    let focused = !snapshot.capabilities.foreground || observed.foreground_pid == Some(pid);
    let afk = settings.afk_timeout_secs > 0 && observed.idle_seconds >= settings.afk_timeout_secs;
    let paused_by_user = state.session_paused.load(std::sync::atomic::Ordering::Acquire);
    let counting = (if vn_app.media_type == "game" { focused } else { !settings.vn_require_focus || focused }) && !afk && !paused_by_user;

    let content_id = vn_app.media_id.clone();
    let (lines, chars) = state.db.text_totals(&content_id);
    let baseline = state.db.get_text_state(&content_id);

    if let Some(session) = snapshot.vn.as_mut() {
        session.is_adult_image = vn_app.is_adult_image;
        session.counting = counting;
        session.paused_by_user = paused_by_user;
        if counting {
            session.credit_elapsed(elapsed);
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
    apply_vn_timer_edit(&app.state::<AppState>(), snapshot);
    let Some(session) = snapshot.vn.take() else {
        return;
    };
    let state = app.state::<AppState>();
    let minutes = session.elapsed_secs / 60;
    if state.hooker.read().await.content_id.as_deref() == Some(session.content_id.as_str()) {
        let _ = state.hooker_tx.send(crate::state::HookerCommand::Disconnect);
    }

    let _ = app.emit(
        EVT_VN_ENDED,
        json!({
            "contentId": session.content_id,
            "title": session.media_title,
            "minutes": minutes,
            "chars": session.session_chars,
        }),
    );

    if minutes == 0 && (session.media_type == "game" || session.session_chars == 0) {
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
    if session.media_type != "game" && session.session_chars > 0 {
        payload["chars"] = json!(session.session_chars);
    }
    payload["_detected"] = json!({
        "source": if session.media_type == "game" { "game" } else { "vn" },
        "exe": session.exe,
        "lines": session.session_lines,
        "seconds": session.elapsed_secs,
        "image": session.image,
    });

    let lang = settings.language.as_str();
    let summary = format!(
        "{} — {}",
        session.media_title,
        crate::i18n::session_summary(lang, minutes, session.session_chars)
    );

    if settings.vn_confirm_before_log || session.media_type == "game" {
        sync::add_pending(app, &payload, if session.media_type == "game" { "game" } else { "vn" });
        notify(app, settings, crate::i18n::session_ended(lang), &summary);
    } else {
        sync::enqueue_log(app, &payload);
        if session.media_type != "game" {
            queue_history(app, &session.content_id, &session, minutes);
        }
        notify(app, settings, crate::i18n::session_logged(lang), &summary);
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
    let logged_at = crate::db::now_iso();
    let payload = json!({
        "contentId": content_id,
        "entry": {
            "isShared": false,
            "connectedUsersCount": 0,
            "linesLogged": session.session_lines,
            "charactersLogged": session.session_chars,
            "readingSpeed": speed,
            "sessionSeconds": session.elapsed_secs,
            "loggedAt": logged_at,
        },
        "minutes": minutes,
    });
    let _ = state
        .db
        .enqueue(&sync::new_id(), "texthooker_history", &payload.to_string());
    let _ = state.db.add_text_history(&crate::db::TextHistoryEntry {
        id: sync::new_id(), content_id: content_id.to_string(), logged_at,
        chars: session.session_chars, lines: session.session_lines,
        seconds: session.elapsed_secs as i64, speed, intelligence: None,
    });
}

// ── helpers ────────────────────────────────────────────────────────────

/// Ask the player where it is, off the tick. A named pipe or a local HTTP call
/// is fast but not instant, and a hung player must not freeze detection.
fn request_playback(app: &AppHandle, player: &str) {
    let player = player.to_string();
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        // One probe in flight at a time.
        if state
            .playback_probing
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return;
        }

        let settings = state.settings_snapshot().await;
        let result = crate::playback::probe(&player, &settings).await;
        *state.playback.write().await = result.map(|playback| PlaybackSnapshot {
            player: player.clone(),
            playback,
        });

        state
            .playback_probing
            .store(false, std::sync::atomic::Ordering::Release);
    });
}

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
    crate::notifications::show(app, title, body);
}

#[cfg(test)]
mod observation_tests {
    use super::*;

    #[test]
    fn active_window_summary_excludes_companion_windows_and_preserves_focus() {
        let own_pid = std::process::id();
        let mut observed = Observed {
            windows: Vec::new(),
            foreground_process: Some("renamed-companion.exe".into()),
            foreground_title: Some("Settings".into()),
            idle_seconds: 0,
            foreground_pid: Some(own_pid),
            running: HashMap::new(),
            media: Vec::new(),
        };
        assert_eq!(observed.active_window_summary(), (None, None));
        assert_eq!(observed.foreground_pid, Some(own_pid));

        observed.foreground_pid = Some(own_pid + 1);
        observed.foreground_process = Some("game.exe".into());
        observed.foreground_title = Some("Game".into());
        assert_eq!(observed.active_window_summary(), (Some("game.exe".into()), Some("Game".into())));

        observed.foreground_pid = None;
        observed.foreground_process = None;
        observed.foreground_title = None;
        assert_eq!(observed.active_window_summary(), (None, None));
    }

    #[test]
    fn credited_time_is_the_real_gap_between_ticks_up_to_a_cap() {
        assert_eq!(credited_elapsed(Duration::from_millis(1030), 1000), Duration::from_millis(1030));
        assert_eq!(credited_elapsed(Duration::from_millis(760), 750), Duration::from_millis(760));
        // Suspend, or a tick stuck behind something slow.
        assert_eq!(credited_elapsed(Duration::from_secs(3600), 1000), Duration::from_secs(5));
        assert_eq!(credited_elapsed(Duration::from_secs(3600), 5000), Duration::from_secs(15));
    }

    #[test]
    fn sub_second_ticks_add_up_to_wall_clock_time() {
        let mut session: VnSession = serde_json::from_value(json!({
            "exe": "game.exe", "content_id": "v1", "media_title": "VN", "media_type": "vn",
            "elapsed_secs": 0, "counting": true, "started_at": "launch",
            "session_chars": 0, "session_lines": 0
        })).unwrap();
        // One minute at the slider's 500 ms and 750 ms steps, which the old
        // per-tick rounding credited as two minutes and 80 seconds.
        for _ in 0..120 { session.credit_elapsed(Duration::from_millis(500)); }
        assert_eq!(session.elapsed_secs, 60);
        for _ in 0..80 { session.credit_elapsed(Duration::from_millis(750)); }
        assert_eq!(session.elapsed_secs, 120);
        // 1250 ms used to round down to a single second.
        for _ in 0..48 { session.credit_elapsed(Duration::from_millis(1250)); }
        assert_eq!(session.elapsed_secs, 180);
        assert_eq!(session.elapsed_carry_ms, 0);
    }

    #[test]
    fn observation_includes_the_running_executable_path() {
        let expected = std::env::current_exe().unwrap();
        let mut system = System::new();
        let observed = observe(&mut system);
        let process = observed.running.values().flatten()
            .find(|process| process.pid == std::process::id())
            .expect("The current process must be observed");
        assert_eq!(process.image_path.as_ref(), Some(&expected));
    }
}
