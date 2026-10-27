//! Optional, Windows-only LunaHook bridge for registered games.
//!
//! The process injector is a separate helper executable so remote-process APIs
//! and the injected hook engine stay out of the Tauri process. The helper emits
//! length-safe, base64-framed UTF-8 records on stdout; every captured line then
//! reuses the normal texthooker ingestion path.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use crate::state::{NativeGameHookPhase, NativeGameHookStatus, NativeHookCandidate};

#[derive(Debug, Clone)]
pub struct GameHookTarget {
    pub content_id: String,
    pub exe: String,
    pub pid: u32,
    pub image_path: Option<PathBuf>,
    pub selected_hook_code: Option<String>,
}

#[derive(Clone, Default)]
pub struct NativeGameHookManager {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    status: NativeGameHookStatus,
    failed_pid: Option<u32>,
    retry_after: Option<Instant>,
    generation: u64,
    websocket_session: Option<(String, u32)>,
    #[cfg(windows)]
    active: Option<ActiveHook>,
}

#[cfg(windows)]
struct ActiveHook {
    pid: u32,
    content_id: String,
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
}

impl NativeGameHookManager {
    pub fn status(&self) -> NativeGameHookStatus {
        let inner = lock(&self.inner);
        let mut status = inner.status.clone();
        status.websocket_selected = inner.uses_websocket();
        status
    }

    pub fn set_websocket(&self, content_id: &str, enabled: bool) {
        let mut inner = lock(&self.inner);
        if inner.status.content_id.as_deref() == Some(content_id) {
            inner.websocket_session = if enabled {
                inner.status.pid.map(|pid| (content_id.to_string(), pid))
            } else { None };
        }
    }

    #[cfg(windows)]
    pub fn reconcile(&self, app: &AppHandle, target: Option<GameHookTarget>) {
        use std::io::BufReader;
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut inner = lock(&self.inner);
        inner.clear_source_if_changed(target.as_ref());
        let target_pid = target.as_ref().map(|entry| entry.pid);

        if let Some(active) = inner.active.as_mut() {
            if Some(active.pid) == target_pid
                && target.as_ref().is_some_and(|target| target.content_id == active.content_id)
            {
                let pid = active.pid;
                match active.child.try_wait() {
                    Ok(None) => return,
                    Ok(Some(exit)) => {
                        inner.active.take();
                        inner.failed_pid = Some(pid);
                        inner.retry_after = Some(Instant::now() + Duration::from_secs(10));
                        inner.status = NativeGameHookStatus {
                            content_id: target.as_ref().map(|entry| entry.content_id.clone()),
                            phase: NativeGameHookPhase::Error,
                            exe: target.as_ref().map(|entry| entry.exe.clone()),
                            pid: Some(pid),
                            message: inner.status.message.clone().or_else(|| {
                                Some(format!(
                                    "Native hook helper exited unexpectedly (code {}).",
                                    exit.code()
                                        .map(|code| code.to_string())
                                        .unwrap_or_else(|| "unknown".into())
                                ))
                            }),
                            hooks: inner.status.hooks.clone(),
                            selected_hook_code: inner.status.selected_hook_code.clone(),
                            websocket_selected: false,
                        };
                        return;
                    }
                    Err(error) => {
                        inner.active.take();
                        inner.failed_pid = Some(pid);
                        inner.retry_after = Some(Instant::now() + Duration::from_secs(10));
                        inner.status = NativeGameHookStatus {
                            content_id: target.as_ref().map(|entry| entry.content_id.clone()),
                            phase: NativeGameHookPhase::Error,
                            exe: target.as_ref().map(|entry| entry.exe.clone()),
                            pid: Some(pid),
                            message: Some(format!("Could not check native hook helper: {error}")),
                            hooks: inner.status.hooks.clone(),
                            selected_hook_code: inner.status.selected_hook_code.clone(),
                            websocket_selected: false,
                        };
                        return;
                    }
                }
            }
        }

        if inner.active.is_some() {
            inner.generation = inner.generation.wrapping_add(1).max(1);
            request_stop(inner.active.take().expect("checked above"));
        }

        let Some(target) = target else {
            inner.failed_pid = None;
            inner.retry_after = None;
            inner.status = NativeGameHookStatus::default();
            return;
        };
        if inner.failed_pid == Some(target.pid) {
            if inner
                .retry_after
                .is_some_and(|retry| Instant::now() < retry)
            {
                return;
            }
        }
        inner.failed_pid = None;
        inner.retry_after = None;

        let Some(image_path) = target.image_path.as_deref() else {
            set_error(
                &mut inner,
                &target,
                "Windows did not provide the game's executable path.",
            );
            return;
        };
        let arch = match executable_architecture(image_path) {
            Ok(arch) => arch,
            Err(error) => {
                set_error(&mut inner, &target, &error);
                return;
            }
        };
        let Some(helper) = helper_path(app, &arch) else {
            set_error(
                &mut inner,
                &target,
                &format!(
                    "Native hook helper for {arch} is missing. Build with the Windows C++ tools."
                ),
            );
            return;
        };

        inner.generation = inner.generation.wrapping_add(1).max(1);
        let generation = inner.generation;
        inner.status = NativeGameHookStatus {
            content_id: Some(target.content_id.clone()),
            phase: NativeGameHookPhase::Starting,
            exe: Some(target.exe.clone()),
            pid: Some(target.pid),
            message: None,
            hooks: Vec::new(),
            selected_hook_code: target.selected_hook_code.clone(),
            websocket_selected: false,
        };

        let mut command = Command::new(helper);
        command
            .arg("--pid")
            .arg(target.pid.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000); // CREATE_NO_WINDOW
        if let Some(code) = target.selected_hook_code.as_deref().and_then(saved_hook_code) {
            use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
            command.arg("--hook").arg(URL_SAFE_NO_PAD.encode(code));
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                set_error(
                    &mut inner,
                    &target,
                    &format!("Could not start native hook helper: {error}"),
                );
                return;
            }
        };
        let stdout = child.stdout.take();
        let stdin = child.stdin.take();
        if stdout.is_none() || stdin.is_none() {
            let _ = child.kill();
            let _ = child.wait();
            set_error(
                &mut inner,
                &target,
                "Could not open the native hook helper's local IPC pipes.",
            );
            return;
        }
        inner.active = Some(ActiveHook {
            pid: target.pid,
            content_id: target.content_id,
            child,
            stdin,
        });

        use tauri::Manager;
        if !inner.uses_websocket() {
            let _ = app.state::<crate::state::AppState>().hooker_tx.send(crate::state::HookerCommand::Disconnect);
        }

        let manager = self.clone();
        let app = app.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout.expect("checked above"));
            for record in std::io::BufRead::lines(reader) {
                let Ok(record) = record else { break };
                manager.handle_record(&app, generation, &record);
            }
        });
    }

    #[cfg(not(windows))]
    pub fn reconcile(&self, _app: &AppHandle, target: Option<GameHookTarget>) {
        let mut inner = lock(&self.inner);
        inner.clear_source_if_changed(target.as_ref());
        if let Some(target) = target {
            inner.status = NativeGameHookStatus {
                content_id: Some(target.content_id),
                phase: NativeGameHookPhase::Unavailable,
                exe: Some(target.exe),
                pid: Some(target.pid),
                message: Some("Native game hooking is available on Windows only.".into()),
                hooks: Vec::new(),
                selected_hook_code: target.selected_hook_code,
                websocket_selected: false,
            };
        } else {
            inner.status = NativeGameHookStatus::default();
        }
    }

    #[cfg(windows)]
    pub fn shutdown(&self) {
        let mut inner = lock(&self.inner);
        if let Some(active) = inner.active.take() {
            inner.generation = inner.generation.wrapping_add(1).max(1);
            stop_child(active);
        }
        inner.status = NativeGameHookStatus::default();
        inner.failed_pid = None;
        inner.retry_after = None;
    }

    pub fn select_hook(&self, exe: &str, hook_code: &str) -> Result<(), String> {
        let mut inner = lock(&self.inner);
        if !inner
            .status
            .exe
            .as_deref()
            .is_some_and(|active_exe| active_exe.eq_ignore_ascii_case(exe))
        {
            return Err("The selected hook does not belong to the active game.".into());
        }
        if !inner
            .status
            .hooks
            .iter()
            .any(|candidate| candidate.selection_key == hook_code)
        {
            return Err("The selected hook is no longer available.".into());
        }
        inner.status.selected_hook_code = Some(hook_code.to_string());
        Ok(())
    }

    #[cfg(windows)]
    fn handle_record(&self, app: &AppHandle, generation: u64, record: &str) {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;

        if let Some(state) = record.strip_prefix("NTDC_STATUS:") {
            let phase = match state {
                "starting" => NativeGameHookPhase::Starting,
                "attached" => NativeGameHookPhase::Attached,
                "stopped" => NativeGameHookPhase::Inactive,
                _ => return,
            };
            update_status(&self.inner, generation, |status| {
                status.phase = phase;
                if phase == NativeGameHookPhase::Inactive {
                    status.exe = None;
                    status.pid = None;
                    status.message = None;
                }
            });
            return;
        }
        if let Some(encoded) = record.strip_prefix("NTDC_SOURCE:") {
            let mut fields = encoded.splitn(4, '|');
            let Some(selection_key) = fields.next().and_then(decode_field) else { return };
            let Some(hook_code) = fields.next().and_then(decode_field) else { return };
            let Some(name) = fields.next().and_then(decode_field) else { return };
            let sample = fields.next().and_then(decode_field).filter(|value| !value.is_empty());
            update_status(&self.inner, generation, |status| {
                upsert_hook(&mut status.hooks, NativeHookCandidate {
                    selection_key,
                    hook_code,
                    name,
                    sample,
                });
            });
            return;
        }
        if let Some(encoded) = record.strip_prefix("NTDC_ERROR:") {
            let message = URL_SAFE_NO_PAD
                .decode(encoded)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_else(|| "The native hook helper failed.".into());
            use tauri::Manager;
            let translated = {
                let state = app.state::<crate::state::AppState>();
                let settings = state.settings.try_read().ok();
                let lang = settings.as_ref().map(|settings| settings.language.as_str()).unwrap_or("en");
                crate::i18n::saved_hook_error(lang, &message).map(str::to_string)
            };
            let message = translated.unwrap_or(message);
            update_status(&self.inner, generation, |status| {
                status.phase = NativeGameHookPhase::Error;
                status.message = Some(message);
            });
            return;
        }
        let Some(encoded) = record.strip_prefix("NTDC_TEXT:") else {
            return;
        };
        let Some((encoded_code, encoded_text)) = encoded.split_once('|') else { return };
        let Some(selection_key) = decode_field(encoded_code) else { return };
        let Some(text) = decode_field(encoded_text) else { return };
        if text.trim().is_empty() {
            return;
        }
        let content_id = {
            let inner = lock(&self.inner);
            if inner.generation != generation
                || inner.uses_websocket()
                || !selection_matches(inner.status.selected_hook_code.as_deref(), &selection_key)
            {
                return;
            }
            let Some(content_id) = inner.status.content_id.clone() else { return };
            content_id
        };
        update_status(&self.inner, generation, |status| {
            status.phase = NativeGameHookPhase::Receiving;
            status.message = None;
        });
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            crate::texthooker::ingest(
                &app,
                text.trim(),
                crate::texthooker::IngestSource::NativeHook { content_id: &content_id },
            )
            .await;
        });
    }
}

impl Inner {
    fn uses_websocket(&self) -> bool {
        self.websocket_session.as_ref().is_some_and(|(content_id, pid)|
            self.status.content_id.as_deref() == Some(content_id) && self.status.pid == Some(*pid))
    }

    fn clear_source_if_changed(&mut self, target: Option<&GameHookTarget>) {
        if !target.is_some_and(|target| self.websocket_session.as_ref()
            .is_some_and(|(content_id, pid)| *content_id == target.content_id && *pid == target.pid)) {
            self.websocket_session = None;
        }
    }
}

#[cfg(windows)]
fn decode_field(encoded: &str) -> Option<String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn selection_matches(selected: Option<&str>, source: &str) -> bool {
    let Some(selected) = selected else { return false };
    if selected.contains('\u{1f}') {
        selected == source
    } else {
        // Preserve selections saved before split contexts were included.
        source.split('\u{1f}').next() == Some(selected)
    }
}

fn saved_hook_code(selection: &str) -> Option<&str> {
    selection.split('\u{1f}').next().filter(|code| !code.trim().is_empty())
}

fn upsert_hook(hooks: &mut Vec<NativeHookCandidate>, candidate: NativeHookCandidate) {
    if candidate.hook_code.is_empty() {
        return;
    }
    if let Some(existing) = hooks.iter_mut().find(|hook| hook.selection_key == candidate.selection_key) {
        if !candidate.name.is_empty() {
            existing.name = candidate.name;
        }
        if candidate.sample.as_ref().is_some_and(|sample| !sample.is_empty()) {
            existing.sample = candidate.sample;
        }
        return;
    }
    if hooks.len() < 128 {
        hooks.push(candidate);
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    #[test]
    fn switching_to_websocket_preserves_the_saved_hook_and_can_restore_native_input() {
        let manager = NativeGameHookManager::default();
        lock(&manager.inner).status = NativeGameHookStatus {
            content_id: Some("v17".into()), pid: Some(17),
            phase: NativeGameHookPhase::Receiving, selected_hook_code: Some("saved".into()),
            ..Default::default()
        };
        manager.set_websocket("v18", true);
        assert!(manager.status().is_active_for(Some("v17")));
        manager.set_websocket("v17", true);
        assert!(manager.status().websocket_selected);
        assert!(!manager.status().is_active_for(Some("v17")));
        assert_eq!(manager.status().selected_hook_code.as_deref(), Some("saved"));
        manager.set_websocket("v17", false);
        assert!(manager.status().is_active_for(Some("v17")));
    }

    #[test]
    fn waiting_for_game_text_does_not_block_switching_sources() {
        for phase in [NativeGameHookPhase::Starting, NativeGameHookPhase::Attached] {
            let manager = NativeGameHookManager::default();
            lock(&manager.inner).status = NativeGameHookStatus {
                content_id: Some("v17".into()),
                pid: Some(17),
                phase,
                ..Default::default()
            };
            manager.set_websocket("v17", true);
            assert!(manager.status().websocket_selected);
            assert!(!manager.status().is_active_for(Some("v17")));
            assert!(manager.status().selected_hook_code.is_none());
            manager.set_websocket("v17", false);
            assert!(manager.status().is_active_for(Some("v17")));
        }
    }

    #[test]
    fn source_choice_expires_when_the_game_closes_or_restarts() {
        let mut inner = Inner { websocket_session: Some(("v17".into(), 17)), ..Default::default() };
        let mut target = GameHookTarget { content_id: "v17".into(), exe: "game.exe".into(), pid: 17, image_path: None, selected_hook_code: None };
        inner.clear_source_if_changed(Some(&target));
        assert!(inner.websocket_session.is_some());
        target.pid = 18;
        inner.clear_source_if_changed(Some(&target));
        assert!(inner.websocket_session.is_none());
        inner.websocket_session = Some(("v17".into(), 18));
        inner.clear_source_if_changed(None);
        assert!(inner.websocket_session.is_none());
    }

    #[test]
    fn only_the_saved_source_is_accepted() {
        let dialogue = "HS@123:game.exe\u{1f}1\u{1f}0";
        let names = "HS@123:game.exe\u{1f}2\u{1f}0";
        assert!(!selection_matches(None, dialogue));
        assert!(selection_matches(Some(dialogue), dialogue));
        assert!(!selection_matches(Some(dialogue), names));
        assert!(selection_matches(Some("HS@123:game.exe"), dialogue));
    }

    #[test]
    fn restoration_uses_the_hook_code_without_the_saved_split_context() {
        assert_eq!(saved_hook_code("HBX0@67540:まじこい.exe\u{1f}0\u{1f}0"), Some("HBX0@67540:まじこい.exe"));
        assert_eq!(saved_hook_code("HS@123:game.exe"), Some("HS@123:game.exe"));
        assert_eq!(saved_hook_code("\u{1f}0\u{1f}0"), None);
    }

    #[test]
    fn previews_keep_split_sources_separate_and_update_their_own_sample() {
        let mut hooks = Vec::new();
        for (key, sample) in [("dialogue", "First line"), ("names", "Name"), ("dialogue", "Next line")] {
            upsert_hook(&mut hooks, NativeHookCandidate {
                selection_key: key.into(),
                hook_code: "HS@123:game.exe".into(),
                name: "Engine".into(),
                sample: Some(sample.into()),
            });
        }
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].sample.as_deref(), Some("Next line"));
        assert_eq!(hooks[1].sample.as_deref(), Some("Name"));
    }
}

#[cfg(windows)]
fn request_stop(active: ActiveHook) {
    std::thread::spawn(move || stop_child(active));
}

#[cfg(windows)]
fn stop_child(mut active: ActiveHook) {
    use std::io::Write;
    if let Some(mut input) = active.stdin.take() {
        let _ = input.write_all(b"stop\n");
        let _ = input.flush();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
    loop {
        match active.child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            _ => break,
        }
    }
    let _ = active.child.kill();
    let _ = active.child.wait();
}

#[cfg(windows)]
fn helper_path(app: &AppHandle, arch: &str) -> Option<PathBuf> {
    use tauri::Manager;

    let relative = PathBuf::from("game-hook").join(arch);
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|root| root.join(&relative).join("ntdc_game_hook.exe"));
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(&relative)
        .join("ntdc_game_hook.exe");
    let candidates = [bundled, Some(development)];
    candidates.into_iter().flatten().find(|path| path.is_file())
}

#[cfg(windows)]
fn executable_architecture(path: &std::path::Path) -> Result<&'static str, String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Could not inspect the game executable: {error}"))?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .map_err(|_| "The game's executable has an invalid DOS header.".to_string())?;
    if dos[0..2] != *b"MZ" {
        return Err("The game's executable is not a valid Windows program.".into());
    }
    let pe_offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().unwrap()) as u64;
    if pe_offset > 16 * 1024 * 1024 {
        return Err("The game's executable has an invalid PE header offset.".into());
    }
    file.seek(SeekFrom::Start(pe_offset))
        .map_err(|error| format!("Could not inspect the game architecture: {error}"))?;
    let mut pe = [0u8; 6];
    file.read_exact(&mut pe)
        .map_err(|_| "The game's executable has a truncated PE header.".to_string())?;
    if pe[0..4] != *b"PE\0\0" {
        return Err("The game's executable has an invalid PE signature.".into());
    }
    match u16::from_le_bytes([pe[4], pe[5]]) {
        0x014c => Ok("x86"),
        0x8664 => Ok("x64"),
        _ => Err("Native game hooking supports x86 and x64 Windows games.".into()),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(windows)]
fn update_status(
    inner: &Mutex<Inner>,
    generation: u64,
    update: impl FnOnce(&mut NativeGameHookStatus),
) {
    let mut inner = lock(inner);
    if inner.generation == generation {
        update(&mut inner.status);
    }
}

#[cfg(windows)]
fn set_error(inner: &mut Inner, target: &GameHookTarget, message: &str) {
    inner.failed_pid = Some(target.pid);
    inner.retry_after = Some(Instant::now() + Duration::from_secs(10));
    inner.status = NativeGameHookStatus {
        content_id: Some(target.content_id.clone()),
        phase: NativeGameHookPhase::Error,
        exe: Some(target.exe.clone()),
        pid: Some(target.pid),
        message: Some(message.to_string()),
        hooks: Vec::new(),
        selected_hook_code: target.selected_hook_code.clone(),
        websocket_selected: false,
    };
}
