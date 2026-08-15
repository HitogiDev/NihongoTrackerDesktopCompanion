use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, State};

use crate::api::MediaHit;
use crate::config::Settings;
use crate::db::{BlacklistEntry, PendingLog, TextHistoryEntry, TextLine, TextState, VnApp};
use crate::state::{AppState, HookerCommand, HookerState, MediaMatch, TrackerSnapshot};
use crate::sync;
use crate::texthooker;

pub type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ── settings & auth ────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings_snapshot().await)
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    state.db.save_settings(&settings).map_err(err)?;
    let (reconnect, language_changed) = {
        let mut current = state.settings.write().await;
        let reconnect = current.texthooker_ws_url != settings.texthooker_ws_url
            || current.texthooker_autoconnect != settings.texthooker_autoconnect;
        let language_changed = current.language != settings.language;
        *current = settings.clone();
        (reconnect, language_changed)
    };

    // The tray menu is built in Rust, so it has to be redrawn by hand.
    if language_changed {
        if let Some(tray) = app.tray_by_id("main-tray") {
            if let Ok(menu) = crate::tray_menu(&app, &settings.language) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    }

    // A new server or key invalidates whatever we matched before.
    state.media_cache.write().await.clear();
    // Saving is the only way back from a revoked key, so let the streak
    // reminder try again and drop the streak it read with the old one.
    state
        .streak_auth_failed
        .store(false, std::sync::atomic::Ordering::Release);
    *state.streak.write().await = Default::default();

    if reconnect {
        let cmd = if settings.texthooker_autoconnect {
            HookerCommand::Connect(settings.texthooker_ws_url.clone())
        } else {
            HookerCommand::Disconnect
        };
        let _ = state.hooker_tx.send(cmd);
    }

    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        let _ = if settings.autostart {
            manager.enable()
        } else {
            manager.disable()
        };
    }
    // Starts, restarts or tears down the presence — including wiping it when
    // the toggle goes off.
    crate::discord::apply_settings(&app, &settings).await;

    let _ = app.emit("settings://changed", &settings);
    Ok(settings)
}

#[tauri::command]
pub async fn verify_api_key(base_url: String, api_key: String) -> CmdResult<Value> {
    let client = crate::api::ApiClient::new(
        format!("{}/api", base_url.trim_end_matches('/')),
        Some(api_key),
    );
    client.verify().await.map_err(err)
}

#[tauri::command]
pub async fn connection_status(state: State<'_, AppState>) -> CmdResult<Value> {
    let settings = state.settings_snapshot().await;
    if !settings.is_authenticated() {
        return Ok(json!({ "ok": false, "reason": "no-api-key" }));
    }
    match state.client().await.verify().await {
        Ok(value) => Ok(json!({ "ok": true, "user": value })),
        Err(e) => Ok(json!({ "ok": false, "reason": e.to_string() })),
    }
}

// ── tracker ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_tracker_state(state: State<'_, AppState>) -> CmdResult<TrackerSnapshot> {
    Ok(state.tracker.read().await.clone())
}

/// Log the episode currently on screen without waiting for the threshold.
#[tauri::command]
pub async fn log_current_anime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Value> {
    let session = state
        .tracker
        .read()
        .await
        .anime
        .clone()
        .ok_or_else(|| "no anime session detected".to_string())?;
    if session.logged {
        return Err("this episode was already logged".into());
    }

    let settings = state.settings_snapshot().await;
    let sent = crate::watcher::commit_anime(&app, &session, &settings, true).await;

    // Keep the snapshot honest so the UI stops offering the button.
    if let Some(current) = state.tracker.write().await.anime.as_mut() {
        if current.key == session.key {
            current.logged = true;
        }
    }
    Ok(json!({ "pending": !sent }))
}

#[derive(Serialize)]
pub struct ProcessEntry {
    pub name: String,
    pub pid: u32,
    pub title: Option<String>,
}

/// Visible windows first (that's what the user recognises), then the rest.
#[tauri::command]
pub fn list_processes() -> CmdResult<Vec<ProcessEntry>> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::new());

    let windows = crate::platform::visible_windows();
    let mut entries: Vec<ProcessEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = Default::default();

    for w in windows {
        if let Some(process) = sys.process(sysinfo::Pid::from_u32(w.pid)) {
            let name = process.name().to_string_lossy().to_ascii_lowercase();
            if seen.insert(name.clone()) {
                entries.push(ProcessEntry {
                    name,
                    pid: w.pid,
                    title: Some(w.title),
                });
            }
        }
    }

    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_ascii_lowercase();
        if seen.insert(name.clone()) {
            entries.push(ProcessEntry {
                name,
                pid: pid.as_u32(),
                title: None,
            });
        }
    }
    Ok(entries)
}

// ── pending confirmations ──────────────────────────────────────────────

#[tauri::command]
pub async fn list_pending(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let rows = state.db.list_pending().map_err(err)?;
    Ok(rows.into_iter().map(pending_to_value).collect())
}

fn pending_to_value(p: PendingLog) -> Value {
    json!({
        "id": p.id,
        "createdAt": p.created_at,
        "source": p.source,
        "payload": serde_json::from_str::<Value>(&p.payload).unwrap_or(Value::Null),
    })
}

/// Accept a parked log — optionally with edits made in the UI.
#[tauri::command]
pub async fn confirm_pending(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    payload: Option<Value>,
) -> CmdResult<Value> {
    let stored = state
        .db
        .get_pending(&id)
        .map_err(err)?
        .ok_or_else(|| "pending log not found".to_string())?;

    let mut body = payload
        .unwrap_or_else(|| serde_json::from_str(&stored.payload).unwrap_or(Value::Null));
    if let Some(obj) = body.as_object_mut() {
        obj.remove("_detected");
    }

    state.db.delete_pending(&id).map_err(err)?;
    let outbox_id = sync::enqueue_log(&app, &body);
    sync::flush(&app).await;
    let _ = app.emit(sync::EVT_PENDING, json!({ "removed": id }));
    Ok(json!({ "outboxId": outbox_id }))
}

#[tauri::command]
pub async fn discard_pending(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    state.db.delete_pending(&id).map_err(err)?;
    let _ = app.emit(sync::EVT_PENDING, json!({ "removed": id }));
    Ok(())
}

// ── outbox ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_outbox(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let rows = state.db.outbox_all(100).map_err(err)?;
    Ok(rows
        .into_iter()
        .map(|item| {
            json!({
                "id": item.id,
                "kind": item.kind,
                "status": item.status,
                "attempts": item.attempts,
                "lastError": item.last_error,
                "createdAt": item.created_at,
                "payload": serde_json::from_str::<Value>(&item.payload).unwrap_or(Value::Null),
            })
        })
        .collect())
}

#[tauri::command]
pub async fn retry_outbox(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.requeue(&id).map_err(err)?;
    sync::flush(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn delete_outbox(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_outbox(&id).map_err(err)
}

#[tauri::command]
pub async fn flush_outbox(app: AppHandle) -> CmdResult<()> {
    sync::flush(&app).await;
    sync::sync_text_lines(&app).await;
    Ok(())
}

// ── logs ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_log(app: AppHandle, payload: Value) -> CmdResult<Value> {
    let id = sync::enqueue_log(&app, &payload);
    sync::flush(&app).await;
    Ok(json!({ "outboxId": id }))
}

#[tauri::command]
pub async fn preview_xp(state: State<'_, AppState>, payload: Value) -> CmdResult<Value> {
    state.client().await.preview_xp(&payload).await.map_err(err)
}

#[tauri::command]
pub async fn recent_logs(state: State<'_, AppState>, limit: Option<u32>) -> CmdResult<Value> {
    let settings = state.settings_snapshot().await;
    let username = settings
        .username
        .clone()
        .ok_or_else(|| "no username configured".to_string())?;
    state
        .client()
        .await
        .recent_logs(&username, limit.unwrap_or(10))
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn ranking(
    state: State<'_, AppState>,
    page: Option<u32>,
    limit: Option<u32>,
    filter: Option<String>,
    time_filter: Option<String>,
    timezone: Option<String>,
) -> CmdResult<Value> {
    state
        .client()
        .await
        .ranking(
            page.unwrap_or(1),
            limit.unwrap_or(25),
            filter.as_deref().unwrap_or("userXp"),
            time_filter.as_deref().unwrap_or("all-time"),
            // The server buckets by day in the caller's timezone; the webview
            // knows it (Intl), Rust would have to guess.
            timezone.as_deref().unwrap_or("UTC"),
        )
        .await
        .map_err(err)
}

/// Whether the streak is about to be lost, for the in-app warning. Shares the
/// reminder loop's cache, so opening the screen repeatedly costs no requests.
#[tauri::command]
pub async fn streak_status(app: AppHandle) -> CmdResult<Option<crate::streak::StreakStatus>> {
    let allowed_age = crate::streak::budget(&app).await;
    crate::streak::refresh(&app, allowed_age).await.map_err(err)
}

#[tauri::command]
pub async fn user_stats(state: State<'_, AppState>) -> CmdResult<Value> {
    let settings = state.settings_snapshot().await;
    let username = settings
        .username
        .clone()
        .ok_or_else(|| "no username configured".to_string())?;
    state.client().await.user_stats(&username).await.map_err(err)
}

// ── Discord Rich Presence ──────────────────────────────────────────────

/// Exactly what Discord would show right now, decided by the same function the
/// watcher uses — the settings preview is the real thing, not a mock-up.
#[tauri::command]
pub async fn discord_preview(state: State<'_, AppState>) -> CmdResult<Value> {
    let settings = state.settings_snapshot().await;
    let blacklist = state.db.discord_blacklist().unwrap_or_default();
    let snapshot = state.tracker.read().await.clone();
    let decision = crate::discord::decide(&snapshot, &settings, &blacklist);
    let connected = state
        .discord
        .read()
        .await
        .as_ref()
        .is_some_and(|d| d.connected.load(std::sync::atomic::Ordering::Acquire));
    Ok(json!({
        "decision": decision,
        "connected": connected,
    }))
}

#[tauri::command]
pub async fn discord_blacklist(state: State<'_, AppState>) -> CmdResult<Vec<BlacklistEntry>> {
    state.db.discord_blacklist().map_err(err)
}

#[tauri::command]
pub async fn add_discord_blacklist(
    state: State<'_, AppState>,
    entry: BlacklistEntry,
) -> CmdResult<Vec<BlacklistEntry>> {
    if entry.key.trim().is_empty() {
        return Err("an entry needs a media id or an executable".into());
    }
    state.db.add_discord_blacklist(&entry).map_err(err)?;
    state.db.discord_blacklist().map_err(err)
}

#[tauri::command]
pub async fn remove_discord_blacklist(
    state: State<'_, AppState>,
    key: String,
) -> CmdResult<Vec<BlacklistEntry>> {
    state.db.delete_discord_blacklist(&key).map_err(err)?;
    state.db.discord_blacklist().map_err(err)
}

// ── media ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn search_media(
    state: State<'_, AppState>,
    query: String,
    media_type: String,
) -> CmdResult<Vec<MediaHit>> {
    state
        .client()
        .await
        .search_media(&query, crate::api::search_type_for(&media_type), 15)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn multi_search_media(
    state: State<'_, AppState>,
    query: String,
) -> CmdResult<Vec<MediaHit>> {
    state.client().await.multi_search(&query, 15).await.map_err(err)
}

/// Pin a detected release title to a specific media entry, so the fuzzy search
/// is skipped from now on.
#[tauri::command]
pub async fn set_title_mapping(
    state: State<'_, AppState>,
    title: String,
    media: MediaMatch,
) -> CmdResult<()> {
    let mut media = media;
    media.manual = true;
    state
        .db
        .set_kv(
            &format!("title_map:{}", title.to_lowercase()),
            &serde_json::to_string(&media).map_err(err)?,
        )
        .map_err(err)?;
    state.media_cache.write().await.clear();
    Ok(())
}

#[tauri::command]
pub async fn list_title_mappings(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let rows = state.db.list_title_map().map_err(err)?;
    Ok(rows
        .into_iter()
        .map(|(title, raw)| {
            json!({
                "title": title,
                "media": serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null),
            })
        })
        .collect())
}

#[tauri::command]
pub async fn delete_title_mapping(state: State<'_, AppState>, title: String) -> CmdResult<()> {
    state
        .db
        .delete_kv(&format!("title_map:{}", title.to_lowercase()))
        .map_err(err)?;
    state.media_cache.write().await.clear();
    Ok(())
}

// ── VN registry ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_vn_apps(state: State<'_, AppState>) -> CmdResult<Vec<VnApp>> {
    state.db.list_vn_apps().map_err(err)
}

#[tauri::command]
pub async fn upsert_vn_app(state: State<'_, AppState>, app_entry: VnApp) -> CmdResult<Vec<VnApp>> {
    state.db.upsert_vn_app(&app_entry).map_err(err)?;
    state.db.list_vn_apps().map_err(err)
}

#[tauri::command]
pub async fn delete_vn_app(state: State<'_, AppState>, exe: String) -> CmdResult<Vec<VnApp>> {
    state.db.delete_vn_app(&exe).map_err(err)?;
    state.db.list_vn_apps().map_err(err)
}

// ── texthooker ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn hooker_state(state: State<'_, AppState>) -> CmdResult<HookerState> {
    Ok(state.hooker.read().await.clone())
}

#[tauri::command]
pub async fn hooker_connect(state: State<'_, AppState>, url: Option<String>) -> CmdResult<()> {
    let url = match url {
        Some(u) if !u.trim().is_empty() => u,
        _ => state.settings_snapshot().await.texthooker_ws_url,
    };
    state
        .hooker_tx
        .send(HookerCommand::Connect(url))
        .map_err(err)
}

#[tauri::command]
pub async fn hooker_disconnect(state: State<'_, AppState>) -> CmdResult<()> {
    state.hooker_tx.send(HookerCommand::Disconnect).map_err(err)
}

/// Point incoming lines at a session (a media contentId, or a local blank one).
#[tauri::command]
pub async fn set_hooker_session(
    app: AppHandle,
    state: State<'_, AppState>,
    content_id: String,
    title: Option<String>,
    media_type: Option<String>,
) -> CmdResult<HookerState> {
    {
        let mut hooker = state.hooker.write().await;
        hooker.content_id = Some(content_id.clone());
        hooker.content_title = title.clone();
    }
    let existing = state.db.get_text_state(&content_id);
    let (lines, chars) = state.db.text_totals(&content_id);
    state
        .db
        .save_text_state(&TextState {
            content_id: content_id.clone(),
            timer_seconds: existing.timer_seconds,
            baseline_chars: if existing.baseline_chars == 0 && existing.baseline_lines == 0 {
                chars
            } else {
                existing.baseline_chars
            },
            baseline_lines: if existing.baseline_lines == 0 && existing.baseline_chars == 0 {
                lines
            } else {
                existing.baseline_lines
            },
            media_type,
            media_title: title,
        })
        .map_err(err)?;

    let snapshot = state.hooker.read().await.clone();
    let _ = app.emit(texthooker::EVT_STATUS, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn hooker_lines(
    state: State<'_, AppState>,
    content_id: String,
    limit: Option<i64>,
) -> CmdResult<Vec<TextLine>> {
    state
        .db
        .lines_for(&content_id, limit.unwrap_or(2000))
        .map_err(err)
}

#[tauri::command]
pub async fn hooker_totals(state: State<'_, AppState>, content_id: String) -> CmdResult<Value> {
    let (lines, chars) = state.db.text_totals(&content_id);
    let st = state.db.get_text_state(&content_id);
    Ok(json!({
        "lines": lines,
        "chars": chars,
        "timerSeconds": st.timer_seconds,
        "baselineChars": st.baseline_chars,
        "baselineLines": st.baseline_lines,
        "sessionChars": (chars - st.baseline_chars).max(0),
        "sessionLines": (lines - st.baseline_lines).max(0),
        "mediaTitle": st.media_title,
        "mediaType": st.media_type,
    }))
}

#[tauri::command]
pub async fn hooker_ingest(app: AppHandle, text: String) -> CmdResult<()> {
    texthooker::ingest(&app, &text, texthooker::IngestSource::Socket).await;
    Ok(())
}

/// The reader owns its clock; ingestion needs to know whether it is running to
/// honour the "allow during pause" and autostart settings.
#[tauri::command]
pub async fn hooker_set_timer_active(state: State<'_, AppState>, active: bool) -> CmdResult<()> {
    state
        .hooker_timer_active
        .store(active, std::sync::atomic::Ordering::Release);
    Ok(())
}

/// Attach a free-form session to a real media entry, carrying its lines over.
#[tauri::command]
pub async fn hooker_attach_media(
    app: AppHandle,
    state: State<'_, AppState>,
    from_content_id: String,
    content_id: String,
    title: String,
    media_type: String,
) -> CmdResult<HookerState> {
    if from_content_id != content_id {
        let previous = state.db.get_text_state(&from_content_id);
        state
            .db
            .reassign_lines(&from_content_id, &content_id)
            .map_err(err)?;

        let target = state.db.get_text_state(&content_id);
        state
            .db
            .save_text_state(&TextState {
                content_id: content_id.clone(),
                timer_seconds: target.timer_seconds.max(previous.timer_seconds),
                baseline_chars: target.baseline_chars,
                baseline_lines: target.baseline_lines,
                media_type: Some(media_type.clone()),
                media_title: Some(title.clone()),
            })
            .map_err(err)?;
    }

    {
        let mut hooker = state.hooker.write().await;
        hooker.content_id = Some(content_id.clone());
        hooker.content_title = Some(title);
    }
    let snapshot = state.hooker.read().await.clone();
    let _ = app.emit(texthooker::EVT_STATUS, &snapshot);
    Ok(snapshot)
}

/// Put a deleted line back (the reader's undo).
#[tauri::command]
pub async fn hooker_restore_line(state: State<'_, AppState>, line: TextLine) -> CmdResult<()> {
    state.db.insert_line(&line).map_err(err)?;
    Ok(())
}

#[tauri::command]
pub async fn hooker_history(
    state: State<'_, AppState>,
    content_id: String,
) -> CmdResult<Vec<TextHistoryEntry>> {
    state.db.text_history(&content_id).map_err(err)
}

#[tauri::command]
pub async fn hooker_delete_line(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_line(&id).map_err(err)
}

#[tauri::command]
pub async fn hooker_clear(state: State<'_, AppState>, content_id: String) -> CmdResult<()> {
    state.db.clear_lines(&content_id).map_err(err)?;
    let mut st = state.db.get_text_state(&content_id);
    st.baseline_chars = 0;
    st.baseline_lines = 0;
    state.db.save_text_state(&st).map_err(err)
}

#[tauri::command]
pub async fn hooker_set_timer(
    state: State<'_, AppState>,
    content_id: String,
    seconds: i64,
) -> CmdResult<()> {
    let mut st = state.db.get_text_state(&content_id);
    st.timer_seconds = seconds.max(0);
    state.db.save_text_state(&st).map_err(err)
}

#[tauri::command]
pub async fn list_text_sessions(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let states = state.db.list_text_states().map_err(err)?;
    Ok(states
        .into_iter()
        .map(|s| {
            let (lines, chars) = state.db.text_totals(&s.content_id);
            json!({
                "contentId": s.content_id,
                "mediaTitle": s.media_title,
                "mediaType": s.media_type,
                "timerSeconds": s.timer_seconds,
                "lines": lines,
                "chars": chars,
                "sessionChars": (chars - s.baseline_chars).max(0),
                "sessionLines": (lines - s.baseline_lines).max(0),
            })
        })
        .collect())
}

/// Turn the accumulated texthooker session into a log and reset the baseline —
/// the same flow as `handleLogSession` in the web client.
#[derive(Deserialize)]
pub struct LogSessionArgs {
    pub content_id: String,
    pub media_type: String,
    pub description: String,
    pub chars: i64,
    pub seconds: i64,
}

#[tauri::command]
pub async fn log_text_session(
    app: AppHandle,
    state: State<'_, AppState>,
    args: LogSessionArgs,
) -> CmdResult<Value> {
    let minutes = (args.seconds / 60).max(0);
    if minutes == 0 && args.chars == 0 {
        return Err("nothing to log yet".into());
    }

    let mut payload = json!({
        "type": args.media_type,
        "mediaId": args.content_id,
        "description": args.description,
        "date": chrono::Utc::now().to_rfc3339(),
    });
    if minutes > 0 {
        payload["time"] = json!(minutes);
    }
    if args.chars > 0 {
        payload["chars"] = json!(args.chars);
    }

    let outbox_id = sync::enqueue_log(&app, &payload);

    let speed = if args.seconds > 0 {
        (args.chars as f64 * 3600.0 / args.seconds as f64).round() as i64
    } else {
        0
    };
    let history = json!({
        "contentId": args.content_id,
        "entry": {
            "isShared": false,
            "connectedUsersCount": 0,
            "charactersLogged": args.chars,
            "readingSpeed": speed,
            "sessionSeconds": args.seconds,
            "loggedAt": chrono::Utc::now().to_rfc3339(),
        }
    });
    let _ = state
        .db
        .enqueue(&sync::new_id(), "texthooker_history", &history.to_string());

    // Baseline moves to "everything so far is logged".
    let (lines, chars) = state.db.text_totals(&args.content_id);

    // Same row locally, so the reader's history works offline.
    let previous = state.db.get_text_state(&args.content_id);
    let _ = state.db.add_text_history(&TextHistoryEntry {
        id: sync::new_id(),
        content_id: args.content_id.clone(),
        logged_at: chrono::Utc::now().to_rfc3339(),
        chars: args.chars,
        lines: (lines - previous.baseline_lines).max(0),
        seconds: args.seconds,
        speed,
    });
    let mut st = state.db.get_text_state(&args.content_id);
    st.baseline_chars = chars;
    st.baseline_lines = lines;
    st.timer_seconds = 0;
    let _ = state.db.save_text_state(&st);

    sync::flush(&app).await;
    Ok(json!({ "outboxId": outbox_id }))
}

// ── misc ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn media_detail(
    state: State<'_, AppState>,
    media_type: String,
    content_id: String,
) -> CmdResult<Value> {
    state
        .client()
        .await
        .media_detail(crate::api::search_type_for(&media_type), &content_id)
        .await
        .map_err(err)
}

/// Pull the server-side texthooker session (lines + timer) for a media entry.
#[tauri::command]
pub async fn remote_text_session(
    state: State<'_, AppState>,
    content_id: String,
) -> CmdResult<Value> {
    state.client().await.get_session(&content_id).await.map_err(err)
}

#[derive(Serialize)]
pub struct InstalledExtension {
    pub folder: String,
    pub name: String,
    pub version: String,
    /// Manifest V3 extensions need a recent WebView2 runtime.
    pub manifest_version: u64,
    /// Chrome-style id, derived from the install path.
    pub id: String,
    /// `chrome-extension://<id>/<options page>`, when the manifest has one.
    pub options_url: Option<String>,
}

/// Chromium derives an unpacked extension's id from its absolute path:
/// SHA-256 of the path, first 16 bytes hex-encoded, then each hex digit mapped
/// `0-f` → `a-p`. On Windows the path is hashed as UTF-16LE. Verified against a
/// real WebView2 profile — this is how we can link to an extension's own pages,
/// since the webview has no toolbar to click.
fn extension_id_for_path(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};

    let bytes: Vec<u8> = if cfg!(windows) {
        path.to_string_lossy()
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect()
    } else {
        path.to_string_lossy().as_bytes().to_vec()
    };

    let digest = Sha256::digest(&bytes);
    digest[..16]
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .map(|nibble| (b'a' + nibble) as char)
        .collect()
}

/// Unpacked extensions sitting in the extensions folder, read straight from
/// their `manifest.json`.
#[tauri::command]
pub fn list_extensions(app: AppHandle) -> CmdResult<Vec<InstalledExtension>> {
    let dir = crate::extensions_dir(&app);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(Vec::new()),
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let folder = entry.file_name().to_string_lossy().to_string();
        let id = extension_id_for_path(&entry.path());
        let options_page = manifest
            .get("options_ui")
            .and_then(|v| v.get("page"))
            .or_else(|| manifest.get("options_page"))
            .and_then(|v| v.as_str())
            .map(|page| format!("chrome-extension://{id}/{page}"));
        // Store-built extensions localise their name; fall back to the folder.
        let name = manifest
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|n| !n.starts_with("__MSG_"))
            .unwrap_or(&folder)
            .to_string();
        found.push(InstalledExtension {
            folder,
            name,
            version: manifest
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string(),
            manifest_version: manifest
                .get("manifest_version")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            id,
            options_url: options_page,
        });
    }
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(found)
}

#[tauri::command]
pub fn extensions_folder(app: AppHandle) -> CmdResult<String> {
    Ok(crate::extensions_dir(&app).to_string_lossy().to_string())
}

/// Open an extension's own settings page in its own window. There is no
/// toolbar in a webview, so this is the only way in.
#[tauri::command]
pub fn open_extension_options(app: AppHandle, url: String) -> CmdResult<()> {
    if !url.starts_with("chrome-extension://") {
        return Err("not an extension URL".into());
    }
    let parsed = url.parse().map_err(|_| "invalid extension URL".to_string())?;

    // Same extension flags as the main window: wry requires webviews that
    // differ on this to use separate data directories.
    tauri::WebviewWindowBuilder::new(
        &app,
        format!("ext-{}", uuid::Uuid::new_v4()),
        tauri::WebviewUrl::External(parsed),
    )
    .title("Extension settings")
    .inner_size(1000.0, 760.0)
    .browser_extensions_enabled(true)
    .extensions_path(crate::extensions_dir(&app))
    .build()
    .map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn open_extensions_folder(app: AppHandle) -> CmdResult<()> {
    let dir = crate::extensions_dir(&app);
    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(err)
}

#[tauri::command]
pub fn platform_supported() -> bool {
    crate::platform::supported()
}

#[tauri::command]
pub fn parse_release_name(name: String) -> Value {
    serde_json::to_value(crate::parser::parse(&name)).unwrap_or(Value::Null)
}
