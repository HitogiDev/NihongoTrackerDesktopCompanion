use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::{
    handshake::server::{Request, Response},
    Message,
};

use crate::db::TextLine;
use crate::state::{AppState, NativeGameHookStatus};

#[derive(Clone)]
pub struct BrowserBridge {
    port: u16,
    token: String,
    origin: String,
    messages: broadcast::Sender<Value>,
}

pub fn reader_url(base: &str, content_id: &str, bridge: Option<&str>) -> Result<String, String> {
    let mut url = reqwest::Url::parse(base).map_err(|error| error.to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Invalid server URL".into());
    }
    let base_path = url
        .path()
        .trim_end_matches('/')
        .trim_end_matches("/api")
        .to_string();
    url.set_path(&format!("{base_path}/texthooker/{content_id}"));
    url.set_query(None);
    url.set_fragment(bridge.map(|_| ""));
    if let Some(bridge) = bridge {
        url.set_fragment(Some(&format!(
            "ntdc={}",
            reqwest::Url::parse(bridge).map_err(|error| error.to_string())?
        )));
    }
    Ok(url.into())
}

pub async fn endpoint(app: &AppHandle, content_id: &str) -> Result<String, String> {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot().await;
    let origin = reqwest::Url::parse(&settings.api_base_url)
        .map_err(|error| error.to_string())?
        .origin()
        .ascii_serialization();
    let mut current = state.browser_bridge.write().await;
    if current
        .as_ref()
        .is_none_or(|bridge| bridge.origin != origin)
    {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|error| error.to_string())?;
        let bridge = BrowserBridge {
            port: listener
                .local_addr()
                .map_err(|error| error.to_string())?
                .port(),
            token: uuid::Uuid::new_v4().to_string(),
            origin,
            messages: broadcast::channel(256).0,
        };
        *current = Some(bridge.clone());
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let app = app.clone();
                let bridge = bridge.clone();
                tauri::async_runtime::spawn(serve(app, bridge, stream));
            }
        });
    }
    let bridge = current.as_ref().expect("initialized above");
    Ok(format!(
        "ws://127.0.0.1:{}/{}/{}",
        bridge.port, bridge.token, content_id
    ))
}

fn authorize(request: &Request, token: &str, origin: &str) -> Option<String> {
    if request
        .headers()
        .get("origin")
        .and_then(|value| value.to_str().ok())
        != Some(origin)
    {
        return None;
    }
    let content_id = request.uri().path().strip_prefix(&format!("/{token}/"))?;
    if content_id.is_empty() || content_id.contains('/') {
        return None;
    }
    Some(content_id.to_string())
}

async fn serve(app: AppHandle, bridge: BrowserBridge, stream: TcpStream) {
    let mut content_id = None;
    let socket = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio_tungstenite::accept_hdr_async(stream, |request: &Request, response: Response| {
            content_id = authorize(request, &bridge.token, &bridge.origin);
            if content_id.is_some() {
                Ok(response)
            } else {
                Err(tokio_tungstenite::tungstenite::http::Response::builder()
                    .status(403)
                    .body(Some("Forbidden".into()))
                    .expect("valid response"))
            }
        }),
    )
    .await;
    let Ok(Ok(socket)) = socket else { return };
    let Some(content_id) = content_id else { return };
    let state = app.state::<AppState>();
    let (mut writer, mut reader) = socket.split();
    let mut messages = bridge.messages.subscribe();
    let status = status_message(&content_id, &state.game_hook.status());
    if writer
        .send(Message::Text(status.to_string()))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            message = messages.recv() => {
                match message {
                    Ok(message) => {
                        let message = if message.get("type").and_then(Value::as_str) == Some("native-status") {
                            let status = state.game_hook.status();
                            status_message(&content_id, &status)
                        } else if message.get("contentId").and_then(Value::as_str) == Some(&content_id) { message }
                        else { continue };
                        if writer.send(Message::Text(message.to_string())).await.is_err() { break; }
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            message = reader.next() => {
                match message {
                    Some(Ok(Message::Text(raw))) => {
                        let Ok(command) = serde_json::from_str::<Value>(&raw) else { continue };
                        if command.get("type").and_then(Value::as_str) == Some("set-source") {
                            let Some(source) = command.get("source").and_then(Value::as_str) else { continue };
                            let result = crate::commands::change_native_input_source(&app, &content_id, source).await;
                            let reply = match result {
                                Ok(status) => {
                                    let hook = if status.content_id.as_deref() == Some(&content_id) { status } else { NativeGameHookStatus::default() };
                                    json!({"type":"source-changed", "contentId":content_id, "source":source, "hook":hook, "requestId":command.get("requestId")})
                                },
                                Err(error) => json!({"type":"source-error", "message":error, "requestId":command.get("requestId")}),
                            };
                            if writer.send(Message::Text(reply.to_string())).await.is_err() { break; }
                            continue;
                        }
                        if command.get("type").and_then(Value::as_str) != Some("select-hook") { continue; }
                        let Some(selection) = command.get("selectionKey").and_then(Value::as_str) else { continue };
                        let status = state.game_hook.status();
                        let result = if status.content_id.as_deref() != Some(&content_id) { Err("The game session is not active.".into()) }
                        else if let Some(exe) = status.exe.as_deref() {
                            state.game_hook.select_hook(exe, selection).and_then(|()| state.db.set_native_hook_code(exe, selection).map_err(|error| error.to_string()))
                        } else { Err("The game session is not active.".into()) };
                        let reply = match result { Ok(()) => status_message(&content_id, &state.game_hook.status()), Err(error) => json!({"type":"selection-error", "message":error}) };
                        if writer.send(Message::Text(reply.to_string())).await.is_err() { break; }
                    },
                    Some(Ok(Message::Ping(data))) => { if writer.send(Message::Pong(data)).await.is_err() { break; } },
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {},
                }
            }
        }
    }
}

fn status_message(content_id: &str, status: &NativeGameHookStatus) -> Value {
    let status = if status.content_id.as_deref() == Some(content_id) {
        status.clone()
    } else {
        NativeGameHookStatus::default()
    };
    json!({ "type":"native-status", "contentId": content_id, "hook": status })
}

pub async fn publish_status(app: &AppHandle, status: &NativeGameHookStatus) {
    if let Some(bridge) = app.state::<AppState>().browser_bridge.read().await.as_ref() {
        let _ = bridge
            .messages
            .send(json!({"type":"native-status", "hook": status}));
    }
}

pub async fn publish_line(app: &AppHandle, line: &TextLine) {
    if let Some(bridge) = app.state::<AppState>().browser_bridge.read().await.as_ref() {
        let _ = bridge
            .messages
            .send(json!({"type":"native-line", "contentId": line.content_id, "line":line}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_uses_configured_server_and_keeps_local_token_out_of_requests() {
        let url = reqwest::Url::parse(
            &reader_url(
                "http://localhost:5173/api",
                "v17",
                Some("ws://127.0.0.1:9000/token/v17"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(url.path(), "/texthooker/v17");
        assert_eq!(url.query(), None);
        assert_eq!(url.fragment(), Some("ntdc=ws://127.0.0.1:9000/token/v17"));
    }

    #[test]
    fn bridge_requires_token_and_configured_site_origin() {
        let request = Request::builder()
            .uri("/secret/v17")
            .header("Origin", "https://nihongotracker.app")
            .body(())
            .unwrap();
        assert_eq!(
            authorize(&request, "secret", "https://nihongotracker.app"),
            Some("v17".into())
        );
        assert_eq!(
            authorize(&request, "wrong", "https://nihongotracker.app"),
            None
        );
        assert_eq!(
            authorize(&request, "secret", "https://untrusted.example"),
            None
        );
    }

    #[test]
    fn browser_session_cannot_display_another_games_hooks() {
        let status = NativeGameHookStatus { content_id: Some("v17".into()), selected_hook_code: Some("saved".into()), ..Default::default() };
        assert_eq!(status_message("v18", &status)["hook"]["selected_hook_code"], Value::Null);
        assert_eq!(status_message("v17", &status)["hook"]["selected_hook_code"], "saved");
    }
}
