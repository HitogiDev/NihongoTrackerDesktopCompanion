//! Self-update from GitHub Releases.
//!
//! The release workflow (`.github/workflows/release.yml`) publishes signed
//! bundles and a `latest.json` manifest; `tauri.conf.json` points the updater at
//! `releases/latest/download/latest.json` and holds the public key, so only
//! builds signed with our private key are ever installed.
//!
//! Checks run off every other loop: once shortly after start, then every six
//! hours, and whenever the user asks from Settings. A found update is announced
//! once per version (notification + `updater://available`); installing it is
//! always the user's call.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::state::AppState;

pub const EVT_AVAILABLE: &str = "updater://available";
pub const EVT_PROGRESS: &str = "updater://progress";

const KV_NOTIFIED: &str = "updater:notified_version";
const FIRST_CHECK: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// Release notes from the GitHub release body.
    pub notes: Option<String>,
    pub date: Option<String>,
    /// False for installs the updater cannot replace (a .deb or .rpm, where
    /// the package manager owns the files): the UI links to the release instead.
    pub can_install: bool,
}

/// The update found by the last check, kept so installing does not re-check.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

/// Linux packages from .deb/.rpm are owned by the package manager; only the
/// AppImage can replace itself. Windows and macOS bundles always can.
fn can_self_install() -> bool {
    !cfg!(target_os = "linux") || std::env::var_os("APPIMAGE").is_some()
}

fn info(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone().filter(|notes| !notes.trim().is_empty()),
        date: update.date.map(|date| date.to_string()),
        can_install: can_self_install(),
    }
}

/// What the last check found, without asking GitHub again: a window that opens
/// after the background check still learns about the update.
pub fn pending(app: &AppHandle) -> Option<UpdateInfo> {
    app.try_state::<Pending>()?.0.lock().unwrap().as_ref().map(info)
}

/// Asks GitHub for a newer release. `Ok(None)` means up to date.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let found = update.as_ref().map(info);
    *app.state::<Pending>().0.lock().unwrap() = update;
    if let Some(found) = &found {
        let _ = app.emit(EVT_AVAILABLE, found);
    }
    Ok(found)
}

/// Downloads and installs the update from the last check, then restarts. On
/// Windows the installer replaces the running app, so everything that must not
/// be left behind (the game hook, the Discord presence) is stopped first.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    // Cloned, not taken: a failed download can be retried without a new check.
    let update = app.state::<Pending>().0.lock().unwrap().clone();
    let Some(update) = update else { return Err("errors.noUpdate".into()) };
    if !can_self_install() {
        return Err("errors.updateManual".into());
    }
    let mut downloaded: u64 = 0;
    let progress = app.clone();
    let finished = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit(EVT_PROGRESS, json!({ "downloaded": downloaded, "total": total }));
            },
            move || {
                let _ = finished.emit(EVT_PROGRESS, json!({ "done": true }));
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    crate::discord::shutdown(app);
    app.state::<AppState>().game_hook.shutdown();
    app.restart();
}

/// Background check, announcing each new version once.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            let settings = app.state::<AppState>().settings_snapshot().await;
            if settings.auto_update_check {
                match check(&app).await {
                    Ok(Some(found)) => announce(&app, &settings, &found),
                    Ok(None) => {}
                    // Offline, rate-limited or no release yet: try again later.
                    Err(error) => log::info!("Update check failed: {error}"),
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

fn announce(app: &AppHandle, settings: &crate::config::Settings, found: &UpdateInfo) {
    let state = app.state::<AppState>();
    if state.db.get_kv(KV_NOTIFIED).as_deref() == Some(found.version.as_str()) {
        return;
    }
    let _ = state.db.set_kv(KV_NOTIFIED, &found.version);
    if settings.notifications {
        let lang = settings.language.as_str();
        crate::notifications::show(app, crate::i18n::update_title(lang), &crate::i18n::update_body(lang, &found.version));
    }
}
