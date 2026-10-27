use tauri::{AppHandle, Emitter, Manager};

use crate::db::MinedSentence;
use crate::state::AppState;

pub const EVT_MINED: &str = "mining://changed";

pub fn mine_line(app: &AppHandle, line_id: &str) -> Result<MinedSentence, String> {
    let state = app.state::<AppState>();
    let line = state.db.line_by_id(line_id).map_err(|e| e.to_string())?
        .ok_or_else(|| "errors.lineNotFound".to_string())?;
    if crate::texthooker::count_japanese(&line.text) == 0 {
        return Err("errors.noJapanese".into());
    }
    let media_title = state.db.get_text_state(&line.content_id).media_title;
    let sentence = state.db.mine_line(&line, media_title.as_deref()).map_err(|e| e.to_string())?;
    let _ = app.emit(EVT_MINED, &sentence);
    Ok(sentence)
}

pub async fn mine_latest(app: &AppHandle) -> Result<MinedSentence, String> {
    let state = app.state::<AppState>();
    let content_id = state.hooker.read().await.content_id.clone().unwrap_or_else(|| "local".into());
    let line = state.db.latest_line(&content_id).map_err(|e| e.to_string())?
        .ok_or_else(|| "errors.noLine".to_string())?;
    let sentence = mine_line(app, &line.id)?;
    if sentence.screenshot_path.is_none() {
        let exe = state.tracker.read().await.vn.as_ref()
            .filter(|session| session.content_id == content_id)
            .map(|session| session.exe.clone());
        if let Some(exe) = exe {
            let app = app.clone();
            let id = sentence.id.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || capture_focused_work(&app, &exe, &id)).await;
            if let Ok(Some(updated)) = state.db.mined_by_id(&sentence.id) {
                return Ok(updated);
            }
        }
    }
    Ok(sentence)
}

/// Screenshot of the game, only while it is the focused window: mining from
/// the reader or the overlay must not capture whatever else is on screen.
fn capture_focused_work(app: &AppHandle, exe: &str, id: &str) -> Option<()> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let foreground = crate::platform::foreground_window()?;
    let mut system = System::new();
    let refresh = ProcessRefreshKind::new().with_exe(UpdateKind::OnlyIfNotSet);
    #[cfg(not(windows))]
    let refresh = refresh.with_cmd(UpdateKind::OnlyIfNotSet);
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[Pid::from_u32(foreground.pid)]), true, refresh);
    let process = system.process(Pid::from_u32(foreground.pid))?;
    if !crate::platform::process_key(process).eq_ignore_ascii_case(exe) {
        return None;
    }
    let folder = app.path().app_data_dir().ok()?.join("mined");
    std::fs::create_dir_all(&folder).ok()?;
    let path = folder.join(format!("{id}.png"));
    if !crate::platform::capture_window(foreground.pid, &foreground.title, &path) {
        return None;
    }
    let state = app.state::<AppState>();
    if state.db.set_mined_screenshot(id, &path.to_string_lossy()).is_err() {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    let _ = app.emit(EVT_MINED, serde_json::json!({ "id": id }));
    Some(())
}
