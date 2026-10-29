use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::MediaHit;
use crate::config::Settings;
use crate::db::{BlacklistEntry, MinedSentence, PendingLog, TextHistoryEntry, TextLine, TextState, VnApp};
use crate::state::{AppState, HookerCommand, HookerState, MediaMatch, NativeGameHookStatus, TrackerSnapshot};
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
pub fn preview_native_hook_filters(
    filters: crate::text_filters::NativeHookTextFilters,
    text: String,
    reduce_repetitions: Option<bool>,
    filter_chinese: Option<bool>,
) -> crate::text_filters::FilterPreview {
    crate::text_filters::preview(&filters, &text, reduce_repetitions.unwrap_or(false), filter_chinese.unwrap_or(false))
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    mut settings: Settings,
) -> CmdResult<Settings> {
    crate::text_filters::CompiledFilters::compile(&settings.native_hook_text_filters)
        .map_err(|error| crate::i18n::native_filter_error(&settings.language, error.line, &error.message))?;
    settings.discord_app_id = "1378024719672016977".into();
    settings.discord_large_image = "nihongotracker".into();
    settings.reading_overlay.font_size = settings.reading_overlay.font_size.clamp(12, 80);
    settings.reading_overlay.normalize_text_style();
    settings.reading_overlay.opacity = settings.reading_overlay.opacity.min(100);
    settings.reading_overlay.idle_opacity = settings.reading_overlay.idle_opacity.min(100);
    settings.reading_overlay.lookup_delay_ms = settings.reading_overlay.lookup_delay_ms.min(2000);
    if !matches!(settings.reading_overlay.font_weight, 400 | 500 | 600 | 700) {
        settings.reading_overlay.font_weight = 400;
    }
    let previous_overlay = state.settings_snapshot().await.reading_overlay;
    let shortcuts_changed = previous_overlay.toggle_shortcut != settings.reading_overlay.toggle_shortcut
        || previous_overlay.interaction_shortcut != settings.reading_overlay.interaction_shortcut;
    let topmost_changed = previous_overlay.always_on_top != settings.reading_overlay.always_on_top;
    state.db.save_settings(&settings).map_err(err)?;
    let (reconnect, language_changed, auth_changed, auto_reconnect_enabled, theme_changed) = {
        let mut current = state.settings.write().await;
        let reconnect = current.texthooker_ws_url != settings.texthooker_ws_url
            || current.texthooker_autoconnect != settings.texthooker_autoconnect;
        let language_changed = current.language != settings.language;
        let auth_changed = current.api_base_url != settings.api_base_url || current.api_key != settings.api_key;
        let auto_reconnect_enabled = !current.auto_reconnect && settings.auto_reconnect;
        let theme_changed = current.theme != settings.theme;
        *current = settings.clone();
        (reconnect, language_changed, auth_changed, auto_reconnect_enabled, theme_changed)
    };

    if theme_changed {
        crate::window_chrome::sync_theme(&app, &settings.theme);
    }

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
    if auth_changed {
        crate::connection::forget(&app);
    }
    // Saving is the only way back from a revoked key, so let the streak
    // reminder try again and drop the streak it read with the old one.
    state
        .streak_auth_failed
        .store(false, std::sync::atomic::Ordering::Release);
    *state.streak.write().await = Default::default();

    if reconnect {
        let session_active = state.tracker.read().await.vn.is_some();
        let reader_visible = app.get_webview_window("texthooker")
            .is_some_and(|window| window.is_visible().unwrap_or(false));
        let cmd = if settings.texthooker_autoconnect && (session_active || reader_visible) {
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

    crate::set_overlay_enabled(&app, settings.overlay_enabled);

    let _ = app.emit("settings://changed", &settings);
    if !settings.reading_overlay.popup_enabled {
        if crate::reading_overlay::snapshot(&app).popup.is_some_and(|popup| popup.mode == "dictionary") {
            crate::reading_overlay::close_popup(&app);
        }
    }
    if shortcuts_changed { crate::reading_overlay::apply_shortcuts(&app, &settings); }
    if topmost_changed {
        for label in [crate::reading_overlay::LABEL, crate::reading_overlay::POPUP_LABEL,
            crate::reading_overlay::SETTINGS_LABEL, "settings"] {
            if let Some(window) = app.get_webview_window(label) {
                window.set_always_on_top(settings.reading_overlay.always_on_top).map_err(err)?;
            }
        }
    }
    if auth_changed || auto_reconnect_enabled {
        let handle = app.clone();
        let base_url = settings.api_base_url.clone();
        let api_key = settings.api_key.clone();
        tauri::async_runtime::spawn(async move {
            // Settings saves each keystroke in the URL field. Only verify the
            // final value after typing settles.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let latest = handle.state::<AppState>().settings_snapshot().await;
            if latest.api_base_url == base_url && latest.api_key == api_key {
                let _ = crate::connection::check(&handle).await;
            }
        });
    }
    Ok(settings)
}

#[tauri::command]
pub async fn verify_api_key(base_url: String, api_key: String) -> CmdResult<Value> {
    let client = crate::api::ApiClient::new(
        format!("{}/api", base_url.trim_end_matches('/')),
        Some(api_key),
    );
    let user = client.verify().await.map_err(err)?;
    if crate::connection::username(&user).is_none() {
        return Err("invalid-user-response".into());
    }
    Ok(user)
}

#[tauri::command]
pub async fn connection_status(app: AppHandle, cached: Option<bool>) -> CmdResult<Value> {
    if cached.unwrap_or(false) {
        if let Some(status) = crate::connection::cached(&app) {
            return Ok(status);
        }
    }
    Ok(crate::connection::check(&app).await)
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
    comment: String,
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
    let sent = crate::watcher::commit_anime(&app, &session, &settings, true, &comment).await;

    // Keep the snapshot honest so the UI stops offering the button.
    if let Some(current) = state.tracker.write().await.anime.as_mut() {
        if current.key == session.key {
            current.logged = true;
        }
    }
    Ok(json!({ "pending": !sent }))
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ProcessEntry {
    pub name: String,
    pub pid: u32,
    pub title: Option<String>,
}

/// Visible windows first (that's what the user recognises), then the rest.
#[tauri::command]
pub fn list_processes() -> CmdResult<Vec<ProcessEntry>> {
    let mut sys = System::new();
    let refresh = ProcessRefreshKind::new().with_exe(sysinfo::UpdateKind::OnlyIfNotSet);
    #[cfg(not(windows))]
    let refresh = refresh.with_cmd(sysinfo::UpdateKind::OnlyIfNotSet);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);

    // The same names the watcher matches the library against.
    let windows = crate::platform::visible_windows();
    let mut entries: Vec<ProcessEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = Default::default();

    for w in windows {
        if let Some(process) = sys.process(sysinfo::Pid::from_u32(w.pid)) {
            let name = crate::platform::process_key(process);
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
        let name = crate::platform::process_key(process);
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

/// Copy the whole local database to `path`. Includes the API key.
#[tauri::command]
pub async fn export_database(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    state.db.export_to(std::path::Path::new(&path)).map_err(err)
}

/// Replace the local database with a backup and restart, so every loop and
/// cache starts again from what was restored. The current database is kept
/// next to it first, in case the wrong file was picked.
#[tauri::command]
pub async fn restore_database(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let data_dir = app.path().app_data_dir().map_err(err)?;
    {
        let _drain = state.outbox_flush.lock().await;
        state
            .db
            .export_to(&data_dir.join("companion.before-restore.sqlite3"))
            .map_err(err)?;
        state.db.restore_from(std::path::Path::new(&path)).map_err(err)?;
    }
    app.restart();
}

/// Edit a queued or failed log before it goes out.
#[tauri::command]
pub async fn update_outbox(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    mut payload: Value,
) -> CmdResult<()> {
    if let Some(obj) = payload.as_object_mut() {
        obj.remove("_detected");
    }
    {
        // A drain in flight holds the old payload in memory and would mark the
        // row sent after delivering it, so the edit waits for it to finish.
        let _drain = state.outbox_flush.lock().await;
        let updated = state
            .db
            .update_outbox_log(&id, &payload.to_string())
            .map_err(err)?;
        if !updated {
            return Err("this log was already sent".into());
        }
    }
    let _ = app.emit(sync::EVT_OUTBOX, json!({ "queued": state.db.queued_count() }));
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
    audience: Option<String>,
    medium_type: Option<String>,
    medium_metric: Option<String>,
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
            audience.as_deref().unwrap_or("all"),
            medium_type.as_deref(),
            medium_metric.as_deref(),
        )
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn activity_feed(
    state: State<'_, AppState>,
    scope: Option<String>,
    before: Option<String>,
    limit: Option<u32>,
) -> CmdResult<Value> {
    state
        .client()
        .await
        .activity_feed(
            scope.as_deref().unwrap_or("following"),
            before.as_deref(),
            limit.unwrap_or(10).clamp(1, 50),
        )
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn friends_immersion_presence(state: State<'_, AppState>) -> CmdResult<Value> {
    state.client().await.friends_immersion_presence().await.map_err(err)
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
pub async fn upsert_vn_app(state: State<'_, AppState>, mut app_entry: VnApp) -> CmdResult<Vec<VnApp>> {
    if app_entry.reader_mode == crate::db::ReaderMode::Overlay && app_entry.media_type != "vn" { app_entry.reader_mode = crate::db::ReaderMode::Integrated; }
    if app_entry.reader_mode == crate::db::ReaderMode::Overlay { app_entry.native_hook_enabled = true; }
    state.db.upsert_vn_app(&app_entry).map_err(err)?;
    state.db.list_vn_apps().map_err(err)
}

#[tauri::command]
pub async fn delete_vn_app(state: State<'_, AppState>, exe: String) -> CmdResult<Vec<VnApp>> {
    state.db.delete_vn_app(&exe).map_err(err)?;
    state.db.list_vn_apps().map_err(err)
}

#[tauri::command]
pub async fn select_native_hook(
    state: State<'_, AppState>,
    exe: String,
    hook_code: String,
) -> CmdResult<()> {
    state.game_hook.select_hook(&exe, &hook_code)?;
    state.db.set_native_hook_code(&exe, &hook_code).map_err(err)
}

#[tauri::command]
pub fn launch_program(path: String) -> CmdResult<()> {
    let path = std::path::PathBuf::from(path.trim());
    let has_extension = |wanted: &[&str]| path.extension()
        .is_some_and(|ext| wanted.iter().any(|w| ext.eq_ignore_ascii_case(w)));
    // A macOS app is a folder (`Novel.app`); everything else must be a file.
    let is_app_bundle = cfg!(target_os = "macos") && path.is_dir() && has_extension(&["app"]);
    if !path.is_file() && !is_app_bundle { return Err("errors.programNotFound".into()); }
    #[cfg(windows)]
    let result = {
        if has_extension(&["bat", "cmd"]) { std::process::Command::new("cmd").args(["/C", path.to_string_lossy().as_ref()]).spawn() }
        else { std::process::Command::new(&path).spawn() }
    };
    // `open` launches bundles the way Finder does (and keeps them out of our
    // process group).
    #[cfg(target_os = "macos")]
    let result = if is_app_bundle { std::process::Command::new("open").arg(&path).spawn() }
        else { std::process::Command::new(&path).spawn() };
    // Desktop entries describe how to start the program rather than being it.
    #[cfg(target_os = "linux")]
    let result = if has_extension(&["desktop"]) { std::process::Command::new("gio").arg("launch").arg(&path).spawn() }
        else { std::process::Command::new(&path).spawn() };
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    let result = std::process::Command::new(&path).spawn();
    result.map(|_| ()).map_err(err)
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

pub async fn change_native_input_source(app: &AppHandle, content_id: &str, source: &str) -> CmdResult<NativeGameHookStatus> {
    let state = app.state::<AppState>();
    match source {
        "websocket" => state.game_hook.set_websocket(content_id, true),
        "lunahook" => {
            if state.game_hook.status().content_id.as_deref() != Some(content_id) {
                return Err("No active LunaHook session.".into());
            }
            state.game_hook.set_websocket(content_id, false);
            state.hooker_tx.send(HookerCommand::Disconnect).map_err(err)?;
        },
        _ => return Err("Unknown text input source.".into()),
    }
    let status = state.game_hook.status();
    let snapshot = {
        let mut tracker = state.tracker.write().await;
        tracker.game_hook = status.clone();
        tracker.clone()
    };
    let _ = app.emit(crate::watcher::EVT_TRACKER, &snapshot);
    crate::native_bridge::publish_status(app, &status).await;
    Ok(status)
}

#[tauri::command]
pub async fn set_hooker_input_source(app: AppHandle, content_id: String, source: String) -> CmdResult<NativeGameHookStatus> {
    let status = change_native_input_source(&app, &content_id, &source).await?;
    if source == "websocket" {
        let state = app.state::<AppState>();
        let url = state.settings_snapshot().await.texthooker_ws_url;
        state.hooker_tx.send(HookerCommand::Connect(url)).map_err(err)?;
    }
    Ok(status)
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
            baseline_chars: if content_id == "local"
                || existing.baseline_chars != 0
                || existing.baseline_lines != 0
            {
                existing.baseline_chars
            } else {
                chars
            },
            baseline_lines: if content_id == "local"
                || existing.baseline_lines != 0
                || existing.baseline_chars != 0
            {
                existing.baseline_lines
            } else {
                lines
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
pub fn mine_line(app: AppHandle, line_id: String) -> CmdResult<MinedSentence> {
    crate::mining::mine_line(&app, &line_id)
}

#[tauri::command]
pub async fn mine_latest_line(app: AppHandle) -> CmdResult<MinedSentence> {
    crate::mining::mine_latest(&app).await
}

#[tauri::command]
pub fn mined_sentences(state: State<'_, AppState>) -> CmdResult<Vec<MinedSentence>> {
    state.db.mined_sentences().map_err(err)
}

#[tauri::command]
pub fn delete_mined_sentence(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let image = state.db.mined_by_id(&id).map_err(err)?.and_then(|sentence| sentence.screenshot_path);
    state.db.delete_mined(&id).map_err(err)?;
    if let (Some(path), Ok(folder)) = (image, app.path().app_data_dir()) {
        let image_path = std::path::Path::new(&path);
        if image_path.parent() == Some(folder.join("mined").as_path()) {
            let _ = std::fs::remove_file(image_path);
        }
    }
    let _ = app.emit(crate::mining::EVT_MINED, json!({ "id": id }));
    Ok(())
}

#[tauri::command]
pub fn mined_screenshot(state: State<'_, AppState>, id: String) -> CmdResult<Option<String>> {
    use base64::Engine;
    let path = state.db.mined_by_id(&id).map_err(err)?.and_then(|sentence| sentence.screenshot_path);
    path.map(|path| std::fs::read(path).map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)).map_err(err)).transpose()
}

#[tauri::command]
pub async fn export_mined_to_anki(app: AppHandle, id: String) -> CmdResult<i64> {
    sync::export_mined_to_anki(&app, &id).await
}

#[tauri::command]
pub async fn anki_models(state: State<'_, AppState>) -> CmdResult<Value> {
    sync::anki_models(&state.settings_snapshot().await).await
}

#[tauri::command]
pub async fn anki_model_fields(model: String) -> CmdResult<Value> {
    sync::anki_model_fields(&model).await
}

#[tauri::command]
pub async fn mine_dictionary_entry(state: State<'_, AppState>, word: String, reading: String, meaning: String, sentence: String, line_id: Option<String>, content_id: Option<String>) -> CmdResult<i64> {
    let settings = state.settings_snapshot().await;
    if let Some(line_id) = line_id {
        let (sentence, title) = dictionary_line_context(&state.db, &line_id, content_id.as_deref())?;
        return sync::add_dictionary_note(&settings, &word, &reading, &meaning, &sentence, &title).await;
    }
    let hooker = state.hooker.read().await.clone();
    let latest_sentence = hooker.content_id.as_deref().and_then(|content_id| state.db.latest_line(content_id).ok().flatten()).map(|line| line.text).unwrap_or(sentence);
    sync::add_dictionary_note(&settings, &word, &reading, &meaning, &latest_sentence, hooker.content_title.as_deref().unwrap_or("")).await
}

#[tauri::command]
pub fn hooker_latest_line(state: State<'_, AppState>, content_id: String) -> CmdResult<Option<TextLine>> {
    state.db.latest_line(&content_id).map_err(err)
}

fn dictionary_line_context(db: &crate::db::Db, line_id: &str, content_id: Option<&str>) -> CmdResult<(String, String)> {
    let line = db.line_by_id(line_id).map_err(err)?.ok_or("errors.lineNotFound")?;
    if content_id.is_some_and(|id| id != line.content_id) { return Err("Line belongs to another work".into()); }
    let title = db.get_text_state(&line.content_id).media_title.unwrap_or_default();
    Ok((line.text, title))
}

#[cfg(test)]
mod dictionary_context_tests {
    use super::*;

    #[test]
    fn mining_keeps_the_consulted_sentence_and_work_after_new_lines() {
        let db = crate::db::Db::open(std::path::Path::new(":memory:")).unwrap();
        db.save_text_state(&TextState { content_id: "v1".into(), media_title: Some("First VN".into()), ..Default::default() }).unwrap();
        let original = TextLine { id: "first".into(), content_id: "v1".into(), text: "最初の文章。".into(), chars: 5, created_at: "2026-10-03T12:00:00Z".into(), synced: false };
        db.insert_line(&original).unwrap();
        db.insert_line(&TextLine { id: "next".into(), text: "次の文章。".into(), created_at: "2026-10-03T12:00:01Z".into(), ..original.clone() }).unwrap();
        db.insert_line(&TextLine { id: "other".into(), content_id: "v2".into(), ..original.clone() }).unwrap();
        assert_eq!(dictionary_line_context(&db, "first", Some("v1")).unwrap(), (original.text, "First VN".into()));
        assert!(dictionary_line_context(&db, "first", Some("v2")).is_err());
        assert!(dictionary_line_context(&db, "missing", Some("v1")).is_err());
    }
}

#[tauri::command]
pub fn reading_overlay_state(app: AppHandle) -> crate::reading_overlay::OverlayState {
    crate::reading_overlay::snapshot(&app)
}

#[tauri::command]
pub async fn reading_overlay_action(app: AppHandle, action: String, popup: bool) -> CmdResult<()> {
    match action.as_str() {
        "ready" => crate::reading_overlay::ready(&app, popup),
        "toggle" => crate::reading_overlay::toggle(&app),
        "show" => crate::reading_overlay::show(&app),
        "hide" => crate::reading_overlay::hide(&app),
        "interaction" => crate::reading_overlay::toggle_interaction(&app)?,
        "close-popup" => crate::reading_overlay::close_popup(&app),
        "settings" => {
            crate::reading_overlay::opening_settings(&app);
            if let Err(error) = open_aux_window_inner(&app, crate::reading_overlay::SETTINGS_LABEL).await {
                crate::reading_overlay::settings_closed(&app);
                return Err(error);
            }
        }
        _ => return Err("Unknown overlay action".into()),
    }
    Ok(())
}

#[tauri::command]
pub async fn reading_overlay_popup(app: AppHandle, mode: String, line_id: Option<String>, offset: usize, x: f64, y: f64, vertical: Option<bool>, anchor: Option<crate::reading_overlay::PopupAnchor>) -> CmdResult<()> {
    if mode == "dictionary" && !app.state::<AppState>().settings_snapshot().await.reading_overlay.popup_enabled {
        return Err("Dictionary popup disabled".into());
    }
    crate::reading_overlay::open_popup(&app, &mode, line_id.as_deref(), offset, x, y, vertical.unwrap_or(false), anchor)
}

#[tauri::command]
pub async fn reading_overlay_select_hook(app: AppHandle, pid: u32, content_id: String, started_at: String, hook_code: String) -> CmdResult<()> {
    let view = crate::reading_overlay::snapshot(&app);
    let target = view.target.ok_or("No active VN")?;
    let state = app.state::<AppState>();
    let tracker = state.tracker.read().await.clone();
    if target.pid != pid || target.content_id != content_id || target.started_at != started_at || crate::reading_overlay::selected_session(&tracker, &target).is_none() {
        return Err("Session changed".into());
    }
    state.game_hook.select_hook(&target.exe, &hook_code)?;
    state.db.set_native_hook_code(&target.exe, &hook_code).map_err(err)
}

#[tauri::command]
pub async fn overlay_open_main(app: AppHandle, destination: String) -> CmdResult<()> {
    if destination == "/texthooker" {
        return open_work_texthooker(app, None, false).await;
    }
    crate::open_main_with(&app, "overlay://navigate", &destination);
    Ok(())
}

/// What a freshly built main window was opened for (a route, or `/log`), if
/// anything. Taken once: the page asks right after attaching its listeners.
#[tauri::command]
pub fn take_main_intent(state: State<'_, AppState>) -> Option<String> {
    state.main_intent.lock().unwrap().take()
}

#[tauri::command]
pub async fn open_aux_window(app: AppHandle, kind: String) -> CmdResult<()> {
    open_aux_window_inner(&app, &kind).await
}

#[tauri::command]
pub async fn open_work_texthooker(
    app: AppHandle,
    content_id: Option<String>,
    allow_starting_session: bool,
) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let active_content_id = state.tracker.read().await.vn.as_ref().map(|session| session.content_id.clone());
    let requested_content_id = if allow_starting_session {
        content_id.as_deref()
    } else {
        browser_reader_content(content_id.as_deref(), active_content_id.as_deref())
    };
    let Some(content_id) = requested_content_id.map(str::to_string) else {
        return open_aux_window_inner(&app, "texthooker").await;
    };
    let entry = state.db.list_vn_apps().map_err(err)?.into_iter().find(|entry| entry.media_id == content_id);
    if allow_starting_session && entry.as_ref().is_some_and(|entry| entry.reader_mode == crate::db::ReaderMode::Overlay) {
        return Ok(());
    }
    if let Some(entry) = entry.filter(|entry| entry.reader_mode == crate::db::ReaderMode::Browser) {
        use tauri_plugin_opener::OpenerExt;
        let settings = state.settings_snapshot().await;
        let bridge = if entry.native_hook_enabled {
            Some(crate::native_bridge::endpoint(&app, &content_id).await?)
        } else { None };
        let url = crate::native_bridge::reader_url(&settings.api_base_url, &content_id, bridge.as_deref())?;
        app.opener().open_url(url, None::<&str>).map_err(err)
    } else {
        open_aux_window_inner(&app, "texthooker").await
    }
}

fn browser_reader_content<'a>(requested: Option<&str>, active: Option<&'a str>) -> Option<&'a str> {
    active.filter(|active| requested.is_none_or(|requested| requested == *active))
}

#[cfg(test)]
mod reader_tests {
    use super::browser_reader_content;

    #[test]
    fn closed_game_opens_integrated_reader_even_with_a_previous_session() {
        assert_eq!(browser_reader_content(None, None), None);
        assert_eq!(browser_reader_content(Some("v17"), None), None);
        assert_eq!(browser_reader_content(None, Some("v17")), Some("v17"));
        assert_eq!(browser_reader_content(Some("v17"), Some("v18")), None);
    }
}

// WebView2 window creation must run outside the synchronous IPC callback so
// its initialization can keep processing messages on the UI thread.
async fn open_aux_window_inner(app: &AppHandle, kind: &str) -> CmdResult<()> {
    let (label, width, height, min_width, min_height) = match kind {
        "settings" => ("settings", 900.0, 760.0, 560.0, 480.0),
        "reading-overlay-settings" => ("reading-overlay-settings", 680.0, 720.0, 420.0, 440.0),
        "texthooker" => ("texthooker", 1100.0, 760.0, 760.0, 480.0),
        _ => return Err("unknown companion window".to_string()),
    };

    if let Some(window) = app.get_webview_window(label) {
        window.show().map_err(err)?;
        window.unminimize().map_err(err)?;
        window.set_focus().map_err(err)?;
        if kind == "texthooker" {
            texthooker::autoconnect(app).await;
        }
        return Ok(());
    }

    let settings = app.state::<AppState>().settings_snapshot().await;
    let builder = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::default())
        .title(crate::i18n::aux_window_title(&settings.language, kind))
        .inner_size(width, height)
        .min_inner_size(min_width, min_height)
        .resizable(true)
        .center();
    let builder = crate::window_chrome::themed(builder, &settings.theme);
    let builder = if matches!(kind, "settings" | "reading-overlay-settings") {
        let builder = builder.always_on_top(settings.reading_overlay.always_on_top);
        if let Some(popup) = app.get_webview_window(crate::reading_overlay::POPUP_LABEL) {
            // Owned settings stay above the dictionary without changing the overlay's visibility.
            builder.parent(&popup).map_err(err)?
        } else { builder }
    } else { builder };
    builder.build().map_err(err)?;
    if kind == "texthooker" {
        texthooker::autoconnect(app).await;
    }
    Ok(())
}

#[tauri::command]
pub fn set_session_paused(state: State<'_, AppState>, paused: bool) -> CmdResult<()> {
    state.session_paused.store(paused, std::sync::atomic::Ordering::Release);
    Ok(())
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
pub async fn hooker_context(state: State<'_, AppState>, content_id: String) -> CmdResult<Value> {
    Ok(crate::text_cache::context(&state, &content_id).await)
}

#[tauri::command]
pub fn refresh_hooker_context(app: AppHandle, content_id: String) {
    crate::text_cache::request_refresh(&app, content_id, true);
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

/// Start or switch to a real media session without moving lines out of the
/// free-form session. Each session keeps its own text buffer and timer.
#[tauri::command]
pub async fn hooker_attach_media(
    app: AppHandle,
    state: State<'_, AppState>,
    content_id: String,
    title: String,
    media_type: String,
) -> CmdResult<HookerState> {
    let target = state.db.get_text_state(&content_id);
    state
        .db
        .save_text_state(&TextState {
            content_id: content_id.clone(),
            timer_seconds: target.timer_seconds,
            baseline_chars: target.baseline_chars,
            baseline_lines: target.baseline_lines,
            media_type: Some(media_type),
            media_title: Some(title.clone()),
        })
        .map_err(err)?;

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
    app: AppHandle,
    state: State<'_, AppState>,
    content_id: String,
    seconds: i64,
    update_detected_session: Option<bool>,
) -> CmdResult<()> {
    let mut tracker = state.tracker.write().await;
    let mut st = state.db.get_text_state(&content_id);
    st.timer_seconds = seconds.max(0);
    state.db.save_text_state(&st).map_err(err)?;
    let changed = if update_detected_session == Some(true) {
        tracker.vn.as_mut().filter(|session| session.content_id == content_id).map(|session| {
            let edit = crate::state::VnTimerEdit {
                pid: session.pid, content_id: session.content_id.clone(), started_at: session.started_at.clone(), seconds: st.timer_seconds as u64,
            };
            edit.apply(session);
            *state.vn_timer_edit.lock().unwrap() = Some(edit);
        }).is_some()
    } else { false };
    let snapshot = tracker.clone();
    drop(tracker);
    if changed { let _ = app.emit(crate::watcher::EVT_TRACKER, snapshot); }
    Ok(())
}

#[tauri::command]
pub async fn list_text_sessions(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let mut states = state.db.list_text_states().map_err(err)?;
    if !states.iter().any(|session| session.content_id == "local") {
        states.push(state.db.get_text_state("local"));
    }
    states.sort_by_key(|session| session.content_id != "local");
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
    #[serde(default)]
    pub comment: String,
    pub chars: i64,
    pub seconds: i64,
    pub intelligence: Option<Value>,
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
        "mediaTitle": args.description,
        "description": if args.comment.trim().is_empty() { args.description.as_str() } else { args.comment.trim() },
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
    let logged_at = crate::db::now_iso();
    let previous = state.db.get_text_state(&args.content_id);
    let (lines, chars) = state.db.text_totals(&args.content_id);
    let history = json!({
        "contentId": args.content_id,
        "entry": {
            "isShared": false,
            "connectedUsersCount": 0,
            "charactersLogged": args.chars,
            "readingSpeed": speed,
            "sessionSeconds": args.seconds,
            "linesLogged": (lines - previous.baseline_lines).max(0),
            "loggedAt": logged_at,
        }
    });
    let _ = state
        .db
        .enqueue(&sync::new_id(), "texthooker_history", &history.to_string());

    let _ = state.db.add_text_history(&TextHistoryEntry {
        id: sync::new_id(),
        content_id: args.content_id.clone(),
        logged_at,
        chars: args.chars,
        lines: (lines - previous.baseline_lines).max(0),
        seconds: args.seconds,
        speed,
        intelligence: args.intelligence,
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

// ── offline dictionary ────────────────────────────────────────────────

#[tauri::command]
pub fn dictionary_list(state: State<'_, AppState>) -> CmdResult<Vec<crate::dictionary::DictionarySummary>> {
    Ok(state.dictionaries.list())
}

#[tauri::command]
pub async fn dictionary_import(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<crate::dictionary::DictionarySummary> {
    let dictionaries = state.dictionaries.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dictionaries.import_zip(std::path::Path::new(&path))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn dictionary_remove(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.dictionaries.remove(&id)
}

#[tauri::command]
pub async fn dictionary_lookup(
    state: State<'_, AppState>,
    text: String,
    offset: Option<usize>,
    max_results: Option<usize>,
) -> CmdResult<crate::dictionary::LookupResponse> {
    let dictionaries = state.dictionaries.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dictionaries.lookup(&text, offset.unwrap_or(0), max_results.unwrap_or(12))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn platform_supported() -> bool {
    crate::platform::supported()
}

/// The update found by the last check, if any. No network.
#[tauri::command]
pub fn pending_update(app: AppHandle) -> Option<crate::updater::UpdateInfo> {
    crate::updater::pending(&app)
}

/// Asks GitHub for a newer release now, whatever the background schedule.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> CmdResult<Option<crate::updater::UpdateInfo>> {
    crate::updater::check(&app).await
}

/// Installs the release found by the last check and restarts. Progress goes out
/// as `updater://progress`.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult<()> {
    crate::updater::install(&app).await
}

#[tauri::command]
pub fn platform_capabilities() -> crate::platform::Capabilities {
    crate::platform::capabilities()
}

/// macOS: shows the Screen Recording prompt that unlocks window titles. Returns
/// whether titles are readable now (macOS only applies a grant after restart).
#[tauri::command]
pub fn request_window_titles_permission() -> bool {
    crate::platform::request_window_titles_permission()
}

#[tauri::command]
pub fn parse_release_name(name: String) -> Value {
    serde_json::to_value(crate::parser::parse(&name)).unwrap_or(Value::Null)
}
