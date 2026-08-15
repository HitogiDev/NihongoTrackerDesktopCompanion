//! WebSocket client for Textractor / Agent, mirroring what NihongoTracker's
//! web texthooker does (`HookerScreen.tsx` `handleSocketMessage`): accepts raw
//! text frames or `{"sentence": "..."}` JSON, counts CJK characters only.
//!
//! Lines land in SQLite first, so the reader keeps working with no connection;
//! `sync::sync_text_lines` pushes them to the server when it can.

use std::sync::atomic::Ordering;
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
pub const EVT_TIMER: &str = "texthooker://timer";

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
    ingest(app, &text, IngestSource::Socket).await;
}

/// Where a line came from. The reader has a separate "allow during pause"
/// setting per input, same as the web client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestSource {
    Socket,
    Clipboard,
}

/// Store one line and tell the UI about it.
pub async fn ingest(app: &AppHandle, text: &str, source: IngestSource) {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot().await;

    let timer_active = state.hooker_timer_active.load(Ordering::Acquire);
    let allowed_while_paused = match source {
        IngestSource::Socket => settings.texthooker_allow_line_during_pause,
        IngestSource::Clipboard => settings.texthooker_allow_clipboard_during_pause,
    };
    if !timer_active && !allowed_while_paused {
        return;
    }

    // Autostart: the first line of a sitting can start the clock for you.
    let autostart = match source {
        IngestSource::Socket => settings.texthooker_autostart_by_line,
        IngestSource::Clipboard => settings.texthooker_autostart_by_clipboard,
    };
    if !timer_active && autostart {
        state.hooker_timer_active.store(true, Ordering::Release);
        let _ = app.emit(EVT_TIMER, json!({ "active": true }));
    }

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

/// Watch the clipboard for Japanese text.
///
/// The web texthooker gets a second input path from a browser extension that
/// injects `<p>` elements into the page (`HookerScreen.tsx` MutationObserver).
/// A native app can't be on the receiving end of that, so this is the desktop
/// equivalent — and it covers hooks that only copy to the clipboard.
pub fn spawn_clipboard_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<String> = None;
        // Whatever is already in the clipboard when the watcher starts is not
        // a new line; remember it without ingesting.
        let mut primed = false;

        loop {
            let enabled = {
                let state = app.state::<AppState>();
                state.settings_snapshot().await.texthooker_clipboard
            };

            if enabled {
                match read_clipboard().await {
                    Some(text) if !text.trim().is_empty() => {
                        if !primed {
                            primed = true;
                            last = Some(text);
                        } else if last.as_deref() != Some(text.as_str()) {
                            last = Some(text.clone());
                            // Ignore copies that carry no Japanese at all —
                            // otherwise every Ctrl+C becomes a line.
                            if count_japanese(&text) > 0 {
                                ingest(&app, text.trim(), IngestSource::Clipboard).await;
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                primed = false;
                last = None;
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

/// Clipboard access is blocking and platform-specific; keep it off the runtime.
async fn read_clipboard() -> Option<String> {
    tokio::task::spawn_blocking(|| arboard::Clipboard::new().ok()?.get_text().ok())
        .await
        .ok()
        .flatten()
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
    // Losing the hook can stop the clock, so a crash mid-session doesn't bank
    // an hour of nothing.
    if matches!(status, HookerStatus::Disconnected | HookerStatus::Error) {
        let settings = state.settings_snapshot().await;
        if settings.texthooker_pause_on_disconnect
            && state.hooker_timer_active.swap(false, Ordering::AcqRel)
        {
            let _ = app.emit(EVT_TIMER, json!({ "active": false }));
        }
    }

    let snapshot = state.hooker.read().await.clone();
    let _ = app.emit(EVT_STATUS, &snapshot);
}
