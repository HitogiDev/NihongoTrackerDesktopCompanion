//! Outbox: everything that must reach the server goes through here, so the app
//! keeps working offline and catches up later.

use std::time::Duration;
use base64::Engine;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::ApiError;
use crate::db::TextLine;
use crate::state::AppState;

pub const EVT_OUTBOX: &str = "outbox://changed";
pub const EVT_PENDING: &str = "pending://changed";
pub const EVT_CELEBRATION: &str = "log://celebration";

pub fn spawn_presence_sync(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let settings = state.settings_snapshot().await;
            let snapshot = state.tracker.read().await.clone();
            let current = if settings.share_activity {
                snapshot.vn.as_ref().filter(|session| session.counting).map(|session| (session.media_title.clone(), session.media_type.clone(), session.started_at.clone(), session.elapsed_secs))
                    .or_else(|| snapshot.anime.as_ref().filter(|session| !session.paused).and_then(|session| session.media.as_ref().map(|media| (media.title.clone(), "anime".to_string(), session.started_at.clone(), session.elapsed_secs))))
            } else { None };
            let payload = match current {
                Some((title, media_type, started_at, elapsed)) => json!({"sharing": true, "active": true, "mediaTitle": title, "mediaType": media_type, "startedAt": started_at, "elapsedSeconds": elapsed}),
                None => json!({"sharing": settings.share_activity, "active": false}),
            };
            let _ = state.client().await.update_immersion_presence(&payload).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

async fn anki_request(action: &str, params: Value) -> Result<Value, String> {
    let response = crate::api::local_http().post("http://127.0.0.1:8765").timeout(Duration::from_secs(5)).json(&json!({"action": action, "version": 6, "params": params}))
        .send().await.map_err(|_| "errors.ankiUnavailable".to_string())?;
    let body: Value = response.json().await.map_err(|_| "errors.ankiInvalidResponse".to_string())?;
    if let Some(error) = body.get("error").and_then(Value::as_str) { return Err(error.to_string()); }
    body.get("result").cloned().ok_or_else(|| "errors.ankiInvalidResponse".to_string())
}

pub async fn anki_models(_settings: &crate::config::Settings) -> Result<Value, String> {
    anki_request("modelNames", json!({})).await
}
pub async fn anki_model_fields(model: &str) -> Result<Value, String> {
    anki_request("modelFieldNames", json!({"modelName": model})).await
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub async fn add_dictionary_note(settings: &crate::config::Settings, word: &str, reading: &str, meaning: &str, sentence: &str, media: &str) -> Result<i64, String> {
    let model = settings.anki_model.trim();
    let deck = settings.anki_deck.trim();
    if model.is_empty() || deck.is_empty() { return Err("errors.ankiModelMissing".into()); }
    let names = anki_request("modelFieldNames", json!({"modelName": model})).await?;
    let names = names.as_array().ok_or_else(|| "errors.ankiInvalidResponse".to_string())?;
    let mut fields = serde_json::Map::new();
    for name in names.iter().filter_map(Value::as_str) {
        let source = settings.anki_field_sources.get(name).map(String::as_str).unwrap_or("");
        let value = match source {
            "word" => word,
            "reading" => reading,
            "meaning" => meaning,
            "sentence" => sentence,
            "media" => media,
            _ => "",
        };
        fields.insert(name.to_string(), json!(escape_html(value)));
    }
    let note = json!({"deckName": deck, "modelName": model, "fields": fields, "options": {"allowDuplicate": false}, "tags": ["NihongoTracker"]});
    anki_request("addNote", json!({"note": note})).await?.as_i64().ok_or_else(|| "errors.ankiInvalidResponse".to_string())
}

/// Export a locally mined sentence to the Anki desktop app through AnkiConnect.
/// The sentence remains in SQLite if Anki is closed or the request fails.
pub async fn export_mined_to_anki(app: &AppHandle, id: &str) -> Result<i64, String> {
    let state = app.state::<AppState>();
    let sentence = state.db.mined_by_id(id).map_err(|e| e.to_string())?
        .ok_or_else(|| "errors.sentenceNotFound".to_string())?;
    if let Some(note_id) = sentence.anki_note_id {
        return Ok(note_id);
    }
    let settings = state.settings_snapshot().await;
    let deck = settings.anki_deck.trim();
    if deck.is_empty() {
        return Err("errors.deckMissing".into());
    }
    let names = anki_request("modelFieldNames", json!({"modelName": settings.anki_model})).await?;
    let available = names.as_array().ok_or_else(|| "errors.ankiInvalidResponse".to_string())?;
    let mut fields = serde_json::Map::new();
    for field in available.iter().filter_map(Value::as_str) {
        let source = settings.anki_field_sources.get(field).map(String::as_str).unwrap_or("");
        let value = match source { "word" | "sentence" => sentence.text.as_str(), "reading" | "meaning" => "", "media" => sentence.media_title.as_deref().unwrap_or(""), _ => "" };
        fields.insert(field.to_string(), json!(escape_html(value)));
    }
    let mut note = json!({
        "deckName": deck,
        "modelName": settings.anki_model,
        "fields": fields,
        "options": { "allowDuplicate": false },
        "tags": ["NihongoTracker"]
    });
    if let Some(path) = sentence.screenshot_path.as_deref() {
        if let Ok(bytes) = std::fs::read(path) {
            note["picture"] = json!([{
                "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                "filename": format!("nihongotracker-{}.png", sentence.id),
                "fields": [settings.anki_field_sources.iter().find_map(|(field, source)| (source == "sentence").then_some(field.as_str())).unwrap_or("Front")]
            }]);
        }
    }
    let note_id = anki_request("addNote", json!({ "note": note })).await?.as_i64()
        .ok_or_else(|| "errors.ankiInvalidResponse".to_string())?;
    state.db.mark_mined_exported(id, note_id).map_err(|e| e.to_string())?;
    let _ = app.emit(crate::mining::EVT_MINED, json!({ "id": id }));
    Ok(note_id)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Queue a finished log. Returns the outbox id.
pub fn enqueue_log(app: &AppHandle, payload: &Value) -> String {
    let state = app.state::<AppState>();
    let id = new_id();
    let _ = state.db.enqueue(&id, "log", &payload.to_string());
    let _ = app.emit(EVT_OUTBOX, json!({ "queued": state.db.queued_count() }));
    id
}

/// Park a log until the user confirms it in the UI.
pub fn add_pending(app: &AppHandle, payload: &Value, source: &str) -> String {
    let state = app.state::<AppState>();
    let id = new_id();
    let _ = state.db.add_pending(&id, &payload.to_string(), source);
    let _ = app.emit(EVT_PENDING, json!({ "id": id, "payload": payload, "source": source }));
    id
}

/// Send one queued item. `Ok(())` means it left the machine.
async fn deliver(app: &AppHandle, kind: &str, payload: &str) -> Result<(), ApiError> {
    let state = app.state::<AppState>();
    let client = state.client().await;
    let value: Value = serde_json::from_str(payload).unwrap_or(Value::Null);

    match kind {
        "log" => {
            let created = client.create_log(&value).await?;
            // The server answers with the celebration payload (XP, level-up,
            // rank overtakes) and any achievements the log unlocked. Forward it
            // so the UI can play the same sequence as the web app.
            let celebration = created.get("celebration").cloned();
            let achievements = created.get("newAchievements").cloned();
            if celebration.is_some() || achievements.is_some() {
                let _ = app.emit(
                    EVT_CELEBRATION,
                    json!({
                        "celebration": celebration,
                        "newAchievements": achievements,
                    }),
                );
            }
            Ok(())
        }
        "texthooker_lines" => {
            let content_id = value
                .get("contentId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let lines = value.get("lines").cloned().unwrap_or(Value::Array(vec![]));
            client.push_lines(&content_id, &lines).await?;
            Ok(())
        }
        "texthooker_timer" => {
            let content_id = value
                .get("contentId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let seconds = value.get("timerSeconds").and_then(|v| v.as_i64()).unwrap_or(0);
            client.update_timer(&content_id, seconds).await?;
            Ok(())
        }
        "texthooker_history" => {
            let content_id = value
                .get("contentId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let entry = value.get("entry").cloned().unwrap_or(Value::Null);
            client.add_history(&content_id, &entry).await?;
            Ok(())
        }
        other => Err(ApiError::Http {
            status: 400,
            message: format!("unknown outbox kind: {other}"),
            code: None,
        }),
    }
}

/// Drain the outbox once. Stops early on the first network failure so a dead
/// connection doesn't burn through every queued item.
///
/// Callers wait for a drain already in flight instead of skipping it: the rows
/// are read after the lock, so whatever the caller just queued is still sent.
pub async fn flush(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.settings_snapshot().await.is_authenticated() {
        return;
    }
    let _drain = state.outbox_flush.lock().await;

    let items = match state.db.queued(50) {
        Ok(items) => items,
        Err(_) => return,
    };
    if items.is_empty() {
        return;
    }

    let mut changed = false;
    for item in items {
        match deliver(app, &item.kind, &item.payload).await {
            Ok(()) => {
                let _ = state.db.mark_sent(&item.id);
                if item.kind == "log" {
                    if let Ok(payload) = serde_json::from_str(&item.payload) {
                        crate::text_cache::log_delivered(app, &payload).await;
                    }
                } else if item.kind == "texthooker_history" {
                    if let Ok(payload) = serde_json::from_str::<Value>(&item.payload) {
                        if let Some(id) = payload.get("contentId").and_then(Value::as_str) {
                            crate::text_cache::request_refresh(app, id.to_string(), true);
                        }
                    }
                }
                changed = true;
            }
            Err(err) => {
                let permanent = err.is_permanent() || item.attempts >= 10;
                let _ = state.db.mark_failed(&item.id, &err.to_string(), permanent);
                changed = true;
                if !permanent {
                    // Network problem — try again on the next sweep.
                    break;
                }
            }
        }
    }

    if changed {
        let _ = app.emit(EVT_OUTBOX, json!({ "queued": state.db.queued_count() }));
    }
}

/// Background loop: flush the outbox every 30 s.
pub fn spawn_flusher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            flush(&app).await;
            sync_text_lines(&app).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

/// Push locally stored texthooker lines that never made it to the server.
pub async fn sync_text_lines(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.settings_snapshot().await.is_authenticated() {
        return;
    }
    // The tray's "sync now" can overlap the 30 s loop. The server dedupes line
    // ids, but only after reading the session, so two pushes can still race.
    let _drain = state.outbox_flush.lock().await;
    let lines = match state.db.unsynced_lines(200) {
        Ok(l) if !l.is_empty() => l,
        _ => return,
    };

    // The API is per-session, so group by content id. Blank local-only sessions
    // (no server session yet) are skipped — they sync when they get logged.
    let mut grouped: std::collections::HashMap<String, Vec<TextLine>> = Default::default();
    for line in lines {
        grouped.entry(line.content_id.clone()).or_default().push(line);
    }

    let client = state.client().await;
    for (content_id, batch) in grouped {
        if content_id.is_empty() {
            continue;
        }
        let payload: Vec<Value> = batch
            .iter()
            .map(|l| {
                json!({
                    "id": l.id,
                    "text": l.text,
                    "charsCount": l.chars,
                    "createdAt": l.created_at,
                })
            })
            .collect();
        match client.push_lines(&content_id, &Value::Array(payload)).await {
            Ok(_) => {
                let ids: Vec<String> = batch.iter().map(|l| l.id.clone()).collect();
                let _ = state.db.mark_lines_synced(&ids);
                crate::text_cache::request_refresh(app, content_id, true);
            }
            Err(err) if err.is_permanent() => {
                // Session doesn't exist server-side (blank/local): stop retrying.
                let ids: Vec<String> = batch.iter().map(|l| l.id.clone()).collect();
                let _ = state.db.mark_lines_synced(&ids);
            }
            Err(_) => break,
        }
    }
}
