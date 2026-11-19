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

/// What friends see in "immersing now": the first session that counts, in
/// the same order as Discord (a VN or game, the manga reader, a video).
///
/// A manga session follows Discord's policy (`discord::manga_privacy`): a
/// blacklisted series says nothing at all, and an adult or unlinked one (its
/// title would be a folder name) only says that the user is reading manga.
/// The blacklist is the user's list of works not to publish, whether or not
/// Discord is on.
pub fn immersion_presence(snapshot: &crate::state::TrackerSnapshot, settings: &crate::config::Settings, blacklist: &[crate::db::BlacklistEntry]) -> Value {
    if !settings.share_activity {
        return json!({"sharing": false, "active": false});
    }
    let inactive = json!({"sharing": true, "active": false});
    // The same policy as Discord for every kind of session: a blacklisted work
    // shares nothing, an adult or unresolved one only says "immersing".
    let title_for = |reason: crate::discord::Reason, title: &str| {
        if reason == crate::discord::Reason::Full {
            title.to_string()
        } else {
            crate::i18n::discord_generic(&settings.language).to_string()
        }
    };
    let current = if let Some(session) = snapshot.vn.as_ref().filter(|session| session.counting) {
        match crate::discord::vn_privacy(session, settings, blacklist) {
            Err(_) => return inactive,
            Ok(reason) => Some((title_for(reason, &session.media_title), session.media_type.clone(), session.started_at.clone(), session.elapsed_secs)),
        }
    } else if let Some(manga) = snapshot.manga.as_ref().filter(|session| session.counting) {
        match crate::discord::manga_privacy(manga, settings, blacklist) {
            Err(_) => return inactive,
            Ok(reason) => Some((title_for(reason, manga.title()), "manga".to_string(), manga.started_at.clone(), manga.elapsed_secs)),
        }
    } else if let Some(session) = snapshot.anime.as_ref().filter(|session| !session.paused && session.media.is_some()) {
        match crate::discord::anime_privacy(session, settings, blacklist) {
            Err(_) => return inactive,
            Ok(reason) => {
                let title = session.media.as_ref().map(|media| media.title.as_str()).unwrap_or_default();
                Some((title_for(reason, title), "anime".to_string(), session.started_at.clone(), session.elapsed_secs))
            }
        }
    } else {
        None
    };
    match current {
        Some((title, media_type, started_at, elapsed)) => json!({"sharing": true, "active": true, "mediaTitle": title, "mediaType": media_type, "startedAt": started_at, "elapsedSeconds": elapsed}),
        None => inactive,
    }
}

pub fn spawn_presence_sync(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let settings = state.settings_snapshot().await;
            let snapshot = state.tracker.read().await.clone();
            let blacklist = if settings.share_activity { state.db.discord_blacklist().unwrap_or_default() } else { Vec::new() };
            let payload = immersion_presence(&snapshot, &settings, &blacklist);
            let _ = state.client().await.update_immersion_presence(&payload).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

/// Looks unlinked manga series up on NihongoTracker, one search at a time,
/// and keeps what it finds as suggestions (`manga::suggest`). Never in the
/// watcher's tick and never a link: the user confirms each one. Paced by
/// `suggest::Pace` (one search every 1.5 s, longer pauses on 429 or
/// failures), series on screen first, and a title searched once.
pub fn spawn_manga_suggester(app: AppHandle) {
    use crate::manga::suggest::{self, Outcome, Pace, Step};
    tauri::async_runtime::spawn(async move {
        // Let the app start before the first search.
        tokio::time::sleep(Duration::from_secs(20)).await;
        let mut pace = Pace::default();
        let mut dirty = false;
        let mut last_emit = std::time::Instant::now();
        loop {
            let state = app.state::<AppState>();
            let settings = state.settings_snapshot().await;
            let configured = settings.api_key.as_deref().is_some_and(|key| !key.trim().is_empty());
            let outcome = if !settings.manga_link_suggestions || !configured {
                Outcome::Idle
            } else {
                let priority = state.manga.suggest_priority.lock().unwrap().clone();
                match suggest::prepare(&state.db, &priority) {
                    Ok(Step::Nothing) | Err(_) => Outcome::Idle,
                    Ok(Step::Done { series_id }) => {
                        dirty = true;
                        remove_priority(&state, &series_id);
                        Outcome::Cached
                    }
                    Ok(Step::Search { series_id, query }) => {
                        let client = state.client().await;
                        match client.search_media(&query, suggest::SEARCH_TYPE, suggest::SEARCH_HITS).await {
                            Ok(hits) => {
                                if suggest::finish(&state.db, &series_id, &query, &hits).is_ok() {
                                    dirty = true;
                                }
                                remove_priority(&state, &series_id);
                                Outcome::Searched
                            }
                            Err(ApiError::Http { status: 429, .. }) => Outcome::RateLimited,
                            Err(ApiError::Http { status: 401 | 403, .. }) | Err(ApiError::NotConfigured) => Outcome::Unauthorized,
                            Err(error) => {
                                log::debug!("manga suggestion search failed: {error}");
                                Outcome::Failed
                            }
                        }
                    }
                }
            };
            // The page hears about it every couple of seconds, not per series.
            if dirty && (outcome == Outcome::Idle || last_emit.elapsed() > Duration::from_secs(2)) {
                dirty = false;
                last_emit = std::time::Instant::now();
                let _ = app.emit(crate::manga::suggest::SUGGESTIONS_CHANGED, ());
            }
            let wait = pace.after(outcome);
            if wait.is_zero() {
                tokio::task::yield_now().await;
            } else if Pace::interruptible(outcome) {
                let wake = state.manga.suggest_wake.notified();
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = wake => {}
                }
            } else {
                tokio::time::sleep(wait).await;
            }
        }
    });
}

fn remove_priority(state: &AppState, series_id: &str) {
    state.manga.suggest_priority.lock().unwrap().retain(|id| id != series_id);
}

/// The address to call, normalised the way Yomitan accepts it: a bare
/// `host:port` gets `http://`, and an empty field falls back to the default.
fn anki_url(settings: &crate::config::Settings) -> String {
    let url = settings.anki_connect_url.trim().trim_end_matches('/');
    if url.is_empty() { return "http://127.0.0.1:8765".into(); }
    if url.contains("://") { url.to_string() } else { format!("http://{url}") }
}

async fn anki_send(url: &str, body: &Value) -> Result<reqwest::Response, reqwest::Error> {
    crate::api::local_http().post(url).timeout(Duration::from_secs(5)).json(body).send().await
}

async fn anki_request(settings: &crate::config::Settings, action: &str, params: Value) -> Result<Value, String> {
    let url = anki_url(settings);
    let mut body = json!({"action": action, "version": 6, "params": params});
    let key = settings.anki_api_key.trim();
    if !key.is_empty() { body["key"] = json!(key); }
    let mut result = anki_send(&url, &body).await;
    // AnkiConnect bound to `localhost` may only listen on `::1`, which
    // 127.0.0.1 never reaches; a browser resolves both, so retry the same
    // way before giving up.
    if matches!(&result, Err(error) if error.is_connect()) && url.contains("127.0.0.1") {
        let fallback = url.replacen("127.0.0.1", "localhost", 1);
        if let Ok(response) = anki_send(&fallback, &body).await { result = Ok(response); }
    }
    let response = result.map_err(|error| {
        log::warn!("AnkiConnect {action} at {url} failed: {error:?}");
        // A timeout means something accepted the connection and never
        // answered: Anki is busy, napping (macOS) or blocked by a dialog.
        if error.is_timeout() { "errors.ankiTimeout" } else { "errors.ankiUnavailable" }.to_string()
    })?;
    let body: Value = response.json().await.map_err(|error| {
        log::warn!("AnkiConnect {action} returned non-JSON: {error}");
        "errors.ankiInvalidResponse".to_string()
    })?;
    if let Some(error) = body.get("error").and_then(Value::as_str) { return Err(error.to_string()); }
    body.get("result").cloned().ok_or_else(|| "errors.ankiInvalidResponse".to_string())
}

pub async fn anki_models(settings: &crate::config::Settings) -> Result<Value, String> {
    anki_request(settings, "modelNames", json!({})).await
}
pub async fn anki_model_fields(settings: &crate::config::Settings, model: &str) -> Result<Value, String> {
    anki_request(settings, "modelFieldNames", json!({"modelName": model})).await
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub async fn add_dictionary_note(settings: &crate::config::Settings, word: &str, reading: &str, meaning: &str, sentence: &str, media: &str) -> Result<i64, String> {
    let model = settings.anki_model.trim();
    let deck = settings.anki_deck.trim();
    if model.is_empty() || deck.is_empty() { return Err("errors.ankiModelMissing".into()); }
    let names = anki_request(settings, "modelFieldNames", json!({"modelName": model})).await?;
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
    anki_request(settings, "addNote", json!({"note": note})).await?.as_i64().ok_or_else(|| "errors.ankiInvalidResponse".to_string())
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
    let names = anki_request(&settings, "modelFieldNames", json!({"modelName": settings.anki_model})).await?;
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
    let note_id = anki_request(&settings, "addNote", json!({ "note": note })).await?.as_i64()
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

/// Park an anime episode for confirmation. Another episode of the same show
/// already waiting from the same day absorbs it, so a binge is one log
/// ("2 ep., 34 min") rather than a card per episode.
pub fn add_anime_pending(app: &AppHandle, payload: &Value) {
    add_pending(app, payload, "anime");
    compact_anime_pending(app);
}

/// Fold every waiting anime row into the oldest one it matches. Also run at
/// startup, so rows queued before merging existed join up too.
pub fn compact_anime_pending(app: &AppHandle) {
    let state = app.state::<AppState>();
    let rows = state.db.list_pending().unwrap_or_default();
    let mut kept: Vec<(String, Value)> = Vec::new();
    let mut changed = false;
    for row in rows.into_iter().filter(|row| row.source == "anime") {
        let Ok(payload) = serde_json::from_str::<Value>(&row.payload) else { continue };
        let target = kept.iter_mut().find_map(|(id, existing)| {
            merge_anime_episode(existing, &payload).map(|merged| (id, existing, merged))
        });
        let Some((id, existing, merged)) = target else {
            kept.push((row.id, payload));
            continue;
        };
        // The row it joins was confirmed or discarded meanwhile: leave this one be.
        if !matches!(state.db.update_pending_payload(id, &merged.to_string()), Ok(true)) {
            continue;
        }
        *existing = merged;
        let _ = state.db.delete_pending(&row.id);
        changed = true;
    }
    if changed {
        let _ = app.emit(EVT_PENDING, json!({ "merged": true }));
    }
}

/// `existing` with the episode in `next` added, or `None` when they are not
/// the same show on the same day. Resolved media match by `mediaId`; an
/// unresolved title only matches another unresolved one with the same season.
pub fn merge_anime_episode(existing: &Value, next: &Value) -> Option<Value> {
    let str_of = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
    let detected = |v: &Value, key: &str| v.get("_detected").and_then(|d| d.get(key)).cloned().unwrap_or(Value::Null);

    if existing.get("type")?.as_str()? != "anime" || next.get("type")?.as_str()? != "anime" {
        return None;
    }
    match (str_of(existing, "mediaId"), str_of(next, "mediaId")) {
        (Some(a), Some(b)) if a == b => {}
        (None, None) => {
            let title = str_of(existing, "mediaTitle")?;
            if Some(&title) != str_of(next, "mediaTitle").as_ref()
                || detected(existing, "season") != detected(next, "season")
            {
                return None;
            }
        }
        _ => return None,
    }
    let local_day = |v: &Value| {
        let date = chrono::DateTime::parse_from_rfc3339(v.get("date")?.as_str()?).ok()?;
        Some(date.with_timezone(&chrono::Local).date_naive())
    };
    if local_day(existing)? != local_day(next)? {
        return None;
    }

    let mut episodes: Vec<i64> = match detected(existing, "episodes") {
        Value::Array(list) => list.iter().filter_map(Value::as_i64).collect(),
        _ => detected(existing, "episode").as_i64().into_iter().collect(),
    };
    let next_episode = detected(next, "episode").as_i64();
    if next_episode.is_some_and(|episode| episodes.contains(&episode)) {
        // Already counted (a rewatch the same day): nothing to add.
        return Some(existing.clone());
    }
    episodes.extend(next_episode);
    episodes.sort_unstable();

    let count = |v: &Value, key: &str| v.get(key).and_then(Value::as_u64).unwrap_or(0);
    let mut merged = existing.clone();
    merged["episodes"] = json!(count(existing, "episodes").max(1) + count(next, "episodes").max(1));
    if existing.get("time").is_some() || next.get("time").is_some() {
        merged["time"] = json!(count(existing, "time") + count(next, "time"));
    }
    merged["date"] = next["date"].clone();
    if let Some(detected) = merged.get_mut("_detected").and_then(Value::as_object_mut) {
        detected.insert("episodes".into(), json!(episodes));
        if let Some(file) = next.get("_detected").and_then(|d| d.get("file")) {
            detected.insert("file".into(), file.clone());
        }
    }
    Some(merged)
}

/// Send one queued item. `Ok(())` means it left the machine.
async fn deliver(app: &AppHandle, id: &str, kind: &str, payload: &str) -> Result<(), ApiError> {
    let state = app.state::<AppState>();
    let client = state.client().await;
    let value: Value = serde_json::from_str(payload).unwrap_or(Value::Null);

    match kind {
        "log" => {
            let created = client.create_log(&value, id).await?;
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
        match deliver(app, &item.id, &item.kind, &item.payload).await {
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

#[cfg(test)]
mod merge_tests {
    use super::merge_anime_episode;
    use serde_json::{json, Value};

    fn episode(media_id: Option<&str>, n: i64, date: &str) -> Value {
        let mut payload = json!({
            "type": "anime",
            "description": "Seitokai no Ichizon Lv.2",
            "mediaTitle": "Seitokai no Ichizon Lv.2",
            "episodes": 1,
            "time": 17,
            "date": date,
            "_detected": { "episode": n, "season": null, "file": format!("ep {n}.mkv"), "image": "cover" },
        });
        if let Some(id) = media_id {
            payload["mediaId"] = json!(id);
        }
        payload
    }

    #[test]
    fn two_episodes_of_the_same_show_become_one_log() {
        let first = episode(Some("12345"), 1, "2026-11-10T12:00:00Z");
        let second = episode(Some("12345"), 2, "2026-11-10T12:10:00Z");
        let merged = merge_anime_episode(&first, &second).expect("same show, same day");
        assert_eq!(merged["episodes"], 2);
        assert_eq!(merged["time"], 34);
        assert_eq!(merged["date"], "2026-11-10T12:10:00Z");
        assert_eq!(merged["_detected"]["episodes"], json!([1, 2]));
        assert_eq!(merged["_detected"]["file"], "ep 2.mkv");

        let third = merge_anime_episode(&merged, &episode(Some("12345"), 3, "2026-11-10T12:20:00Z")).unwrap();
        assert_eq!((third["episodes"].clone(), third["time"].clone()), (json!(3), json!(51)));
        assert_eq!(third["_detected"]["episodes"], json!([1, 2, 3]));
    }

    #[test]
    fn different_shows_or_days_stay_apart() {
        let first = episode(Some("12345"), 1, "2026-11-10T12:00:00Z");
        assert!(merge_anime_episode(&first, &episode(Some("999"), 2, "2026-11-10T12:20:00Z")).is_none());
        assert!(merge_anime_episode(&first, &episode(None, 2, "2026-11-10T12:20:00Z")).is_none());
        assert!(merge_anime_episode(&first, &episode(Some("12345"), 2, "2026-11-13T12:00:00Z")).is_none());
    }

    #[test]
    fn unresolved_titles_merge_only_within_a_season() {
        let first = episode(None, 1, "2026-11-10T12:00:00Z");
        assert!(merge_anime_episode(&first, &episode(None, 2, "2026-11-10T12:20:00Z")).is_some());
        let mut other_season = episode(None, 2, "2026-11-10T12:20:00Z");
        other_season["_detected"]["season"] = json!(2);
        assert!(merge_anime_episode(&first, &other_season).is_none());
    }

    #[test]
    fn an_episode_already_counted_adds_nothing() {
        let first = episode(Some("12345"), 1, "2026-11-10T12:00:00Z");
        let merged = merge_anime_episode(&first, &episode(Some("12345"), 1, "2026-11-10T12:30:00Z")).unwrap();
        assert_eq!(merged, first);
    }

    #[test]
    fn logs_without_time_stay_without_time() {
        let mut first = episode(Some("12345"), 1, "2026-11-10T12:00:00Z");
        let mut second = episode(Some("12345"), 2, "2026-11-10T12:20:00Z");
        first.as_object_mut().unwrap().remove("time");
        second.as_object_mut().unwrap().remove("time");
        let merged = merge_anime_episode(&first, &second).unwrap();
        assert!(merged.get("time").is_none());
    }
}

#[cfg(test)]
mod anki_url_tests {
    use super::anki_url;

    fn with(url: &str) -> String {
        anki_url(&crate::config::Settings { anki_connect_url: url.into(), ..Default::default() })
    }

    #[test]
    fn normalises_the_address_like_yomitan() {
        assert_eq!(with(""), "http://127.0.0.1:8765");
        assert_eq!(with("  http://127.0.0.1:8765/ "), "http://127.0.0.1:8765");
        assert_eq!(with("192.168.1.20:8765"), "http://192.168.1.20:8765");
        assert_eq!(with("https://anki.local:9000"), "https://anki.local:9000");
    }
}

#[cfg(test)]
mod presence_tests {
    use super::immersion_presence;
    use crate::config::Settings;
    use crate::discord::tests::{entry, manga, vn};
    use crate::state::TrackerSnapshot;

    fn sharing() -> Settings {
        Settings { share_activity: true, ..Default::default() }
    }

    fn reading(media_id: Option<&str>, is_adult: bool) -> TrackerSnapshot {
        TrackerSnapshot { manga: Some(manga(media_id, is_adult)), ..Default::default() }
    }

    #[test]
    fn a_linked_manga_is_shared_with_its_title() {
        let payload = immersion_presence(&reading(Some("30104"), false), &sharing(), &[]);
        assert_eq!(payload["active"], true);
        assert_eq!(payload["mediaTitle"], "Yotsuba&!");
        assert_eq!(payload["mediaType"], "manga");
        assert_eq!(payload["elapsedSeconds"], 600);
        assert_eq!(payload["startedAt"], "2026-08-12T20:00:00Z");
    }

    #[test]
    fn an_adult_or_unlinked_manga_only_says_manga() {
        for snapshot in [reading(Some("30104"), true), reading(None, false)] {
            let payload = immersion_presence(&snapshot, &sharing(), &[]);
            assert_eq!(payload["active"], true);
            assert_eq!(payload["mediaType"], "manga");
            let title = payload["mediaTitle"].as_str().unwrap();
            assert_eq!(title, crate::i18n::discord_generic("en"));
            assert!(!title.contains("Yotsuba") && !title.contains("第1巻"));
        }
    }

    #[test]
    fn a_blacklisted_manga_shares_nothing() {
        for (snapshot, key) in [(reading(Some("30104"), false), "30104"), (reading(None, true), "manga:ser-1")] {
            let payload = immersion_presence(&snapshot, &sharing(), &[entry(key, "media")]);
            assert_eq!(payload, serde_json::json!({"sharing": true, "active": false}));
        }
    }

    fn playing(is_adult: bool) -> TrackerSnapshot {
        TrackerSnapshot { vn: Some(vn(is_adult)), ..Default::default() }
    }

    #[test]
    fn a_novel_follows_the_same_policy_as_discord() {
        let full = immersion_presence(&playing(false), &sharing(), &[]);
        assert_eq!(full["mediaTitle"], "蒼の彼方のフォーリズム");
        assert_eq!(full["mediaType"], "vn");

        let adult = immersion_presence(&playing(true), &sharing(), &[]);
        assert_eq!(adult["active"], true);
        assert_eq!(adult["mediaTitle"], crate::i18n::discord_generic("en"));

        let mut adult_cover = playing(false);
        adult_cover.vn.as_mut().unwrap().is_adult_image = Some(true);
        assert_eq!(immersion_presence(&adult_cover, &sharing(), &[])["mediaTitle"], crate::i18n::discord_generic("en"));

        let generic = Settings { discord_generic_mode: true, ..sharing() };
        assert_eq!(immersion_presence(&playing(false), &generic, &[])["mediaTitle"], crate::i18n::discord_generic("en"));

        for key in ["v18", "aokana.exe"] {
            let blocked = immersion_presence(&playing(false), &sharing(), &[entry(key, "media")]);
            assert_eq!(blocked, serde_json::json!({"sharing": true, "active": false}));
        }
    }

    #[test]
    fn a_paused_manga_or_sharing_off_is_not_active() {
        let mut paused = reading(Some("30104"), false);
        paused.manga.as_mut().unwrap().counting = false;
        assert_eq!(immersion_presence(&paused, &sharing(), &[])["active"], false);
        let off = immersion_presence(&reading(Some("30104"), false), &Settings::default(), &[]);
        assert_eq!(off, serde_json::json!({"sharing": false, "active": false}));
    }
}
