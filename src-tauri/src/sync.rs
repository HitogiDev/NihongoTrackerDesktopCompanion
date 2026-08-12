//! Outbox: everything that must reach the server goes through here, so the app
//! keeps working offline and catches up later.

use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::ApiError;
use crate::db::TextLine;
use crate::state::AppState;

pub const EVT_OUTBOX: &str = "outbox://changed";
pub const EVT_PENDING: &str = "pending://changed";

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
            client.create_log(&value).await?;
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
pub async fn flush(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.settings_snapshot().await.is_authenticated() {
        return;
    }

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
