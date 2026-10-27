use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Settings;
use crate::db::{Db, TextHistoryEntry};
use crate::state::AppState;

pub const EVT_CONTEXT: &str = "texthooker://context";

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CachedContext {
    cover: Option<String>,
    cover_url: Option<String>,
    total_characters: Option<i64>,
    logged_characters: i64,
    remote_lines: i64,
    remote_characters: i64,
    remote_line_ids: HashSet<String>,
    history: Vec<TextHistoryEntry>,
    updated_at: Option<String>,
}

fn cache_key(settings: &Settings, content_id: &str) -> String {
    let mut identity = DefaultHasher::new();
    settings.api_root().hash(&mut identity);
    settings.api_key.hash(&mut identity);
    format!("text_context:{:x}:{content_id}", identity.finish())
}

fn read(db: &Db, key: &str) -> CachedContext {
    db.get_kv(key)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn merge_history(
    mut remote: Vec<TextHistoryEntry>,
    local: Vec<TextHistoryEntry>,
) -> Vec<TextHistoryEntry> {
    for entry in local {
        let same = remote.iter_mut().find(|candidate| {
            candidate.id == entry.id
                || (candidate.chars == entry.chars
                    && candidate.seconds == entry.seconds
                    && chrono::DateTime::parse_from_rfc3339(&candidate.logged_at)
                        .ok()
                        .zip(chrono::DateTime::parse_from_rfc3339(&entry.logged_at).ok())
                        .is_some_and(|(a, b)| {
                            (a.timestamp_millis() - b.timestamp_millis()).abs() <= 2000
                        }))
        });
        if let Some(candidate) = same {
            if candidate.intelligence.is_none() {
                candidate.intelligence = entry.intelligence;
            }
        } else {
            remote.push(entry);
        }
    }
    remote.sort_by_key(|entry| {
        std::cmp::Reverse(chrono::DateTime::parse_from_rfc3339(&entry.logged_at).ok())
    });
    remote.truncate(100);
    remote
}

pub async fn context(state: &AppState, content_id: &str) -> Value {
    let settings = state.settings_snapshot().await;
    context_from_db(&state.db, &cache_key(&settings, content_id), content_id)
}

fn context_from_db(db: &Db, key: &str, content_id: &str) -> Value {
    let cached = read(db, key);
    let extra = db
        .text_line_counts(content_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|(id, _)| !cached.remote_line_ids.contains(id))
        .collect::<Vec<_>>();
    let history = merge_history(
        cached.history,
        db.text_history(content_id).unwrap_or_default(),
    );
    json!({
        "cover": cached.cover,
        "totalCharacters": cached.total_characters,
        "loggedCharacters": cached.logged_characters + db.queued_log_chars(content_id),
        "lines": cached.remote_lines + extra.len() as i64,
        "characters": cached.remote_characters + extra.iter().map(|(_, chars)| chars).sum::<i64>(),
        "history": history,
        "updatedAt": cached.updated_at,
    })
}

pub fn request_refresh(app: &AppHandle, content_id: String, force: bool) {
    if content_id == "local" || content_id.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let settings = state.settings_snapshot().await;
        if !settings.is_authenticated() {
            return;
        }
        let key = cache_key(&settings, &content_id);
        let previous = read(&state.db, &key);
        if !force
            && previous
                .updated_at
                .as_deref()
                .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
                .is_some_and(|date| {
                    chrono::Utc::now().signed_duration_since(date).num_seconds() < 60
                })
        {
            return;
        }
        {
            let mut tasks = state.text_cache_tasks.write().await;
            if !tasks.insert(key.clone()) {
                if force {
                    tasks.insert(format!("{key}:again"));
                }
                return;
            }
        }
        let client = crate::api::ApiClient::new(settings.api_root(), settings.api_key.clone());
        let session = client.get_session(&content_id).await;
        if let Ok(session) = session {
            let media_type = session
                .pointer("/mediaId/type")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| state.db.get_text_state(&content_id).media_type);
            let (stats, media) = if let Some(media_type) = media_type {
                let (stats, media) = tokio::join!(
                    client.media_stats(&content_id, &media_type),
                    client.media_detail(crate::api::search_type_for(&media_type), &content_id)
                );
                (stats.ok(), media.ok())
            } else {
                (None, None)
            };
            let mut next = snapshot(
                &session,
                stats.as_ref(),
                media.as_ref(),
                &content_id,
                previous,
            );
            if let Some(url) = next.cover_url.as_deref() {
                if !next
                    .cover
                    .as_deref()
                    .is_some_and(|cover| cover.starts_with("data:"))
                {
                    next.cover = client.cache_image(url).await.or(next.cover);
                }
            }
            let current = state.settings_snapshot().await;
            if cache_key(&current, &content_id) == key {
                if let Ok(raw) = serde_json::to_string(&next) {
                    if state.db.set_kv(&key, &raw).is_ok() {
                        let _ = app.emit(EVT_CONTEXT, json!({ "contentId": content_id }));
                    }
                }
            }
        }
        let repeat = {
            let mut tasks = state.text_cache_tasks.write().await;
            tasks.remove(&key);
            tasks.remove(&format!("{key}:again"))
        };
        if repeat {
            request_refresh(&app, content_id, true);
        }
    });
}

fn snapshot(
    session: &Value,
    stats: Option<&Value>,
    media: Option<&Value>,
    content_id: &str,
    mut previous: CachedContext,
) -> CachedContext {
    if let Some(url) = session
        .pointer("/mediaId/coverImage")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .or_else(|| {
            session
                .pointer("/mediaId/contentImage")
                .and_then(Value::as_str)
        })
    {
        if previous.cover_url.as_deref() != Some(url) {
            previous.cover = Some(url.to_string());
        }
        previous.cover_url = Some(url.to_string());
    }
    previous.total_characters = session
        .pointer("/mediaId/jiten/mainDeck/characterCount")
        .and_then(Value::as_i64)
        .filter(|total| *total > 0)
        .or_else(|| {
            media
                .and_then(|media| media.pointer("/jiten/mainDeck/characterCount"))
                .and_then(Value::as_i64)
                .filter(|total| *total > 0)
        })
        .or(previous.total_characters);
    if let Some(chars) = stats
        .and_then(|stats| stats.pointer("/total/characters"))
        .and_then(Value::as_i64)
    {
        previous.logged_characters = chars;
    }
    if let Some(lines) = session.get("lines").and_then(Value::as_array) {
        previous.remote_line_ids = lines
            .iter()
            .filter_map(|line| line.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        previous.remote_lines = lines.len() as i64;
        previous.remote_characters = lines
            .iter()
            .filter_map(|line| line.get("charsCount").and_then(Value::as_i64))
            .sum();
    }
    if let Some(history) = session.get("sessionHistory").and_then(Value::as_array) {
        previous.history = history
            .iter()
            .filter_map(|entry| {
                let mut entry = entry.clone();
                entry["contentId"] = json!(content_id);
                entry["id"] = entry.get("_id").cloned().unwrap_or_else(|| {
                    json!(entry
                        .get("loggedAt")
                        .and_then(Value::as_str)
                        .unwrap_or_default())
                });
                serde_json::from_value(entry).ok()
            })
            .collect();
    }
    previous.updated_at = Some(crate::db::now_iso());
    previous
}

pub async fn refresh_active(app: &AppHandle, force: bool) {
    let state = app.state::<AppState>();
    let visible = state.hooker.read().await.content_id.clone();
    let native = state.game_hook.status().content_id;
    for id in visible.into_iter().chain(native).collect::<HashSet<_>>() {
        request_refresh(app, id, force);
    }
}

pub async fn log_delivered(app: &AppHandle, payload: &Value) {
    let Some(content_id) = payload.get("mediaId").and_then(Value::as_str) else {
        return;
    };
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot().await;
    let key = cache_key(&settings, content_id);
    let mut cached = read(&state.db, &key);
    cached.logged_characters += payload.get("chars").and_then(Value::as_i64).unwrap_or(0);
    if let Ok(raw) = serde_json::to_string(&cached) {
        let _ = state.db.set_kv(&key, &raw);
    }
    let _ = app.emit(EVT_CONTEXT, json!({ "contentId": content_id }));
    request_refresh(app, content_id.to_string(), true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_sqlite_totals_merge_remote_lines_and_queued_logs_without_duplicates() {
        let db = Db::open(std::path::Path::new(":memory:")).unwrap();
        let cached = CachedContext {
            cover: Some("data:image/png;base64,cached".into()),
            total_characters: Some(1000),
            logged_characters: 300,
            remote_lines: 1,
            remote_characters: 90,
            remote_line_ids: HashSet::from(["shared".into()]),
            ..Default::default()
        };
        db.set_kv("cache", &serde_json::to_string(&cached).unwrap())
            .unwrap();
        for (id, chars) in [("shared", 90), ("offline", 10)] {
            db.insert_line(&crate::db::TextLine {
                id: id.into(),
                content_id: "v17".into(),
                text: "日本語".into(),
                chars,
                created_at: crate::db::now_iso(),
                synced: false,
            })
            .unwrap();
        }
        db.enqueue(
            "log",
            "log",
            &json!({"mediaId":"v17", "chars":50}).to_string(),
        )
        .unwrap();
        db.enqueue(
            "other",
            "log",
            &json!({"mediaId":"v18", "chars":200}).to_string(),
        )
        .unwrap();
        let context = context_from_db(&db, "cache", "v17");
        assert_eq!(context["lines"], 2);
        assert_eq!(context["characters"], 100);
        assert_eq!(context["loggedCharacters"], 350);
        assert_eq!(context["cover"], "data:image/png;base64,cached");
        db.mark_sent("log").unwrap();
        assert_eq!(
            context_from_db(&db, "cache", "v17")["loggedCharacters"],
            300
        );
    }

    #[test]
    fn server_metadata_and_history_survive_offline_serialization() {
        let session = json!({"mediaId": {"coverImage": "https://example.com/cover.jpg", "jiten": {"mainDeck": {"characterCount": 1000}}}, "lines": [{"id": "web", "charsCount": 90}], "sessionHistory": [{"_id": "history", "loggedAt": "2026-10-01T12:00:00.000Z", "charactersLogged": 100, "linesLogged": 5, "sessionSeconds": 60, "readingSpeed": 6000}]});
        let cached = snapshot(
            &session,
            Some(&json!({"total": {"characters": 300}})),
            None,
            "v17",
            CachedContext::default(),
        );
        let restored: CachedContext =
            serde_json::from_str(&serde_json::to_string(&cached).unwrap()).unwrap();
        assert_eq!(restored.total_characters, Some(1000));
        assert_eq!(restored.logged_characters, 300);
        assert_eq!(restored.history[0].id, "history");
        assert_eq!(
            restored.cover_url.as_deref(),
            Some("https://example.com/cover.jpg")
        );
        assert!(restored.remote_line_ids.contains("web"));
    }

    #[test]
    fn full_media_supplies_character_count_when_session_jiten_is_missing() {
        let cached = snapshot(
            &json!({"mediaId": {"jiten": null}}),
            None,
            Some(&json!({"jiten": {"mainDeck": {"characterCount": 1430411}}})),
            "v31055",
            CachedContext::default(),
        );
        assert_eq!(cached.total_characters, Some(1430411));
    }

    #[test]
    fn synchronized_history_is_shown_once() {
        let entry = TextHistoryEntry {
            id: "local".into(),
            content_id: "v17".into(),
            logged_at: "2026-10-01T12:00:00.001Z".into(),
            chars: 100,
            lines: 5,
            seconds: 60,
            speed: 6000,
            intelligence: None,
        };
        let mut remote = entry.clone();
        remote.id = "server".into();
        remote.logged_at = "2026-10-01T12:00:00.000Z".into();
        assert_eq!(merge_history(vec![remote], vec![entry]).len(), 1);
    }

    #[test]
    fn cache_is_scoped_to_server_and_account() {
        let mut a = Settings::default();
        a.api_key = Some("first".into());
        let mut b = a.clone();
        b.api_key = Some("second".into());
        assert_ne!(cache_key(&a, "v17"), cache_key(&b, "v17"));
        b = a.clone();
        b.api_base_url = "https://other.example.com".into();
        assert_ne!(cache_key(&a, "v17"), cache_key(&b, "v17"));
    }
}
