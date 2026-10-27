//! One connection check for all webviews. The status is emitted to every window,
//! including a settings window opened while the main window is hidden.

use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::api::ApiClient;
use crate::state::AppState;

pub const EVT_CONNECTION: &str = "connection://changed";

pub(crate) fn username(user: &Value) -> Option<&str> {
    user.get("username")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
}

/// How old a remembered status can be for a window that is only starting up.
/// The background loop re-checks every 15 s and every check is broadcast, so a
/// new window does not need its own request.
const CACHED_STATUS_MAX_AGE: Duration = Duration::from_secs(20);

/// The last status, unless it is stale. Saving new credentials clears it.
pub fn cached(app: &AppHandle) -> Option<Value> {
    let state = app.state::<AppState>();
    let last = state.connection_last.lock().unwrap();
    last.as_ref()
        .filter(|(at, _)| at.elapsed() < CACHED_STATUS_MAX_AGE)
        .map(|(_, status)| status.clone())
}

pub fn forget(app: &AppHandle) {
    *app.state::<AppState>().connection_last.lock().unwrap() = None;
}

fn remember(state: &AppState, status: &Value) {
    *state.connection_last.lock().unwrap() = Some((std::time::Instant::now(), status.clone()));
}

pub async fn check(app: &AppHandle) -> Value {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot().await;
    if !settings.is_authenticated() {
        state.connection_online.store(false, std::sync::atomic::Ordering::Release);
        let status = json!({ "ok": false, "reason": "no-api-key" });
        remember(&state, &status);
        let _ = app.emit(EVT_CONNECTION, &status);
        return status;
    }

    let result = ApiClient::new(settings.api_root(), settings.api_key.clone())
        .verify()
        .await;

    // A previous request must not overwrite the status after the user changes
    // the server or key while it is in flight.
    let mut current = state.settings.write().await;
    if current.api_base_url != settings.api_base_url || current.api_key != settings.api_key {
        return json!({ "ok": false, "reason": "settings-changed" });
    }

    let status = match result {
        Ok(user) => match username(&user) {
            Some(name) => {
                if current.username.as_deref() != Some(name) {
                    let mut updated = current.clone();
                    updated.username = Some(name.to_string());
                    if let Err(error) = state.db.save_settings(&updated) {
                        eprintln!("Failed to save connected username: {error}");
                    }
                    *current = updated.clone();
                    drop(current);
                    let _ = app.emit("settings://changed", updated);
                } else {
                    drop(current);
                }
                json!({ "ok": true, "user": user })
            }
            None => {
                drop(current);
                json!({ "ok": false, "reason": "invalid-user-response" })
            }
        },
        Err(error) => {
            drop(current);
            json!({ "ok": false, "reason": error.to_string() })
        }
    };
    remember(&state, &status);
    let _ = app.emit(EVT_CONNECTION, &status);
    let online = status.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let previously_online = state.connection_online.swap(online, std::sync::atomic::Ordering::AcqRel);
    if online {
        crate::text_cache::refresh_active(app, !previously_online).await;
        let session = state.tracker.read().await.vn.clone();
        if let Some(session) = session.filter(|session| session.media_type == "vn") {
            crate::discord::request_vn_image_rating(app, session.content_id, !previously_online);
        }
    }
    status
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // The first check also fills in usernames stored by older versions.
        let _ = check(&app).await;
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let settings = app.state::<AppState>().settings_snapshot().await;
            if settings.auto_reconnect && settings.is_authenticated() {
                let _ = check(&app).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::username;

    #[test]
    fn current_user_response_is_not_nested() {
        assert_eq!(username(&json!({ "username": "Hitogi" })), Some("Hitogi"));
        assert_eq!(username(&json!({ "user": { "username": "Hitogi" } })), None);
    }
}
