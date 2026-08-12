//! WebSocket client for Textractor / Agent, mirroring what NihongoTracker's
//! web texthooker does (`HookerScreen.tsx` `handleSocketMessage`): accepts raw
//! text frames or `{"sentence": "..."}` JSON, counts CJK characters only.
//!
//! Lines land in SQLite first, so the reader keeps working with no connection;
//! `sync::sync_text_lines` pushes them to the server when it can.

use std::time::Duration;

use futures_util::StreamExt;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_tungstenite::tungstenite::Message;

use crate::db::TextLine;
use crate::state::{AppState, HookerCommand, HookerStatus};

pub const EVT_LINE: &str = "texthooker://line";
pub const EVT_STATUS: &str = "texthooker://status";

static JP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[\p{Han}\p{Hiragana}\p{Katakana}]").unwrap());

/// Same rule as the web client: kana and kanji only, no punctuation or latin.
pub fn count_japanese(text: &str) -> i64 {
    JP.find_iter(text).count() as i64
}

pub fn spawn(app: AppHandle, mut rx: UnboundedReceiver<HookerCommand>) {
    tauri::async_runtime::spawn(async move {
        let mut current: Option<tauri::async_runtime::JoinHandle<()>> = None;

        while let Some(cmd) = rx.recv().await {
            if let Some(handle) = current.take() {
                handle.abort();
            }
            match cmd {
                HookerCommand::Connect(url) => {
                    set_status(&app, HookerStatus::Connecting, Some(url.clone()), None).await;
                    current = Some(tauri::async_runtime::spawn(connection_loop(
                        app.clone(),
                        url,
                    )));
                }
                HookerCommand::Disconnect => {
                    set_status(&app, HookerStatus::Disconnected, None, None).await;
                }
            }
        }
    });
}

async fn connection_loop(app: AppHandle, url: String) {
    loop {
        set_status(&app, HookerStatus::Connecting, Some(url.clone()), None).await;

        match tokio_tungstenite::connect_async(url.as_str()).await {
            Ok((stream, _)) => {
                set_status(&app, HookerStatus::Connected, Some(url.clone()), None).await;
                let (_write, mut read) = stream.split();

                while let Some(message) = read.next().await {
                    match message {
                        Ok(Message::Text(text)) => handle_text(&app, &text).await,
                        Ok(Message::Binary(bytes)) => {
                            if let Ok(text) = String::from_utf8(bytes) {
                                handle_text(&app, &text).await;
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(err) => {
                            set_status(
                                &app,
                                HookerStatus::Error,
                                Some(url.clone()),
                                Some(err.to_string()),
                            )
                            .await;
                            break;
                        }
                    }
                }
                set_status(&app, HookerStatus::Disconnected, Some(url.clone()), None).await;
            }
            Err(err) => {
                set_status(
                    &app,
                    HookerStatus::Error,
                    Some(url.clone()),
                    Some(err.to_string()),
                )
                .await;
            }
        }

        // Textractor is often started after the companion; keep trying.
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn handle_text(app: &AppHandle, raw: &str) {
    let text = serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| {
            v.get("sentence")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| raw.to_string());

    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    ingest(app, &text).await;
}

/// Store one line and tell the UI about it. Shared with the manual "paste"
/// path exposed as a command.
pub async fn ingest(app: &AppHandle, text: &str) {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot().await;

    let content_id = state
        .hooker
        .read()
        .await
        .content_id
        .clone()
        .unwrap_or_else(|| "local".to_string());

    if settings.texthooker_prevent_duplicates && state.db.line_text_exists(&content_id, text) {
        return;
    }

    let line = TextLine {
        id: uuid::Uuid::new_v4().to_string(),
        content_id: content_id.clone(),
        text: text.to_string(),
        chars: count_japanese(text),
        created_at: crate::db::now_iso(),
        synced: false,
    };

    if state.db.insert_line(&line).unwrap_or(false) {
        let (lines, chars) = state.db.text_totals(&content_id);
        let _ = app.emit(
            EVT_LINE,
            json!({
                "line": line,
                "totals": { "lines": lines, "chars": chars },
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::count_japanese;

    #[test]
    fn counts_kana_and_kanji_only() {
        assert_eq!(count_japanese("「あ、これは本だ」"), 6);
        assert_eq!(count_japanese("Hello, world! 123"), 0);
        assert_eq!(count_japanese("カタカナとひらがなと漢字"), 12);
    }
}

pub async fn set_status(
    app: &AppHandle,
    status: HookerStatus,
    url: Option<String>,
    error: Option<String>,
) {
    let state = app.state::<AppState>();
    {
        let mut hooker = state.hooker.write().await;
        hooker.status = status;
        if let Some(url) = url {
            hooker.url = url;
        }
        hooker.last_error = error;
    }
    let snapshot = state.hooker.read().await.clone();
    let _ = app.emit(EVT_STATUS, &snapshot);
}
