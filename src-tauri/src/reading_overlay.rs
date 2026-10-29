use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::config::Settings;
use crate::db::{ReaderMode, TextLine};
use crate::state::{AppState, TrackerSnapshot, VnSession};

pub const EVT_STATE: &str = "reading-overlay://state";
pub const LABEL: &str = "reading-overlay";
pub const POPUP_LABEL: &str = "reading-overlay-popup";
pub const SETTINGS_LABEL: &str = "reading-overlay-settings";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "ReadingOverlayTarget"))]
pub struct Target {
    pub pid: u32,
    pub exe: String,
    pub content_id: String,
    pub title: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "ReadingOverlayPopup"))]
pub struct Popup {
    pub mode: String,
    pub line: Option<TextLine>,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "ReadingOverlayState"))]
pub struct OverlayState {
    pub target: Option<Target>,
    pub line: Option<TextLine>,
    pub visible: bool,
    pub click_through: bool,
    pub toolbar_hovered: bool,
    pub reveal_controls: bool,
    pub popup: Option<Popup>,
    pub shortcut_errors: Vec<String>,
}

#[derive(Default)]
pub struct Runtime {
    pub view: OverlayState,
    desired: bool,
    auto_mode: bool,
    ready: bool,
    popup_ready: bool,
    placed: bool,
    popup_placed: bool,
    registered_shortcuts: Vec<String>,
    settings_opened_from_overlay: bool,
    settings_focus_until: Option<Instant>,
    ignored_cursor: bool,
    manual_show: bool,
    manual_seen_unfocused: bool,
    reveal_until: Option<Instant>,
}

impl Runtime {
    fn set_session(&mut self, target: Option<Target>, automatic: bool) -> bool {
        let changed = self.view.target != target;
        let same_session = match (&self.view.target, &target) {
            (Some(previous), Some(next)) => previous.pid == next.pid && previous.content_id == next.content_id && previous.started_at == next.started_at,
            (None, None) => true,
            _ => false,
        };
        self.view.target = target;
        if !same_session {
            self.view.line = None;
            self.view.popup = None;
            self.view.click_through = false;
            self.view.toolbar_hovered = false;
            self.ignored_cursor = false;
            self.manual_show = false;
            self.manual_seen_unfocused = false;
            self.reveal_until = None;
            self.view.reveal_controls = false;
            self.desired = automatic;
            self.placed = false;
            self.settings_opened_from_overlay = false;
            self.settings_focus_until = None;
        } else if automatic != self.auto_mode {
            self.desired = automatic;
        }
        self.auto_mode = automatic;
        changed
    }

    fn should_show(&self, foreground_pid: Option<u32>, settings_focused: bool, reader_focused: bool, hide_when_unfocused: bool) -> bool {
        self.ready && self.desired && self.view.target.as_ref().is_some_and(|target| {
            self.manual_show || !hide_when_unfocused || foreground_pid == Some(target.pid) || reader_focused || (self.settings_opened_from_overlay && settings_focused)
        })
    }

    fn reveal(&mut self) {
        self.reveal_until = Some(Instant::now() + Duration::from_secs(5));
    }

    fn refresh_reveal(&mut self, now: Instant) -> bool {
        let reveal = self.reveal_until.is_some_and(|until| now < until);
        let changed = self.view.reveal_controls != reveal;
        self.view.reveal_controls = reveal;
        changed
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Geometry {
    x: i32,
    y: i32,
    /// Logical dimensions keep the reading area stable when the monitor scale changes.
    width: u32,
    height: u32,
}

fn runtime(app: &AppHandle) -> &Mutex<Runtime> {
    &app.state::<AppState>().inner().reading_overlay
}

pub fn snapshot(app: &AppHandle) -> OverlayState {
    runtime(app).lock().unwrap().view.clone()
}

pub fn opening_settings(app: &AppHandle) {
    let mut overlay = runtime(app).lock().unwrap();
    overlay.settings_opened_from_overlay = overlay.view.visible;
    overlay.settings_focus_until = overlay.view.visible.then(|| Instant::now() + Duration::from_secs(2));
}

pub fn settings_closed(app: &AppHandle) {
    let mut overlay = runtime(app).lock().unwrap();
    overlay.settings_opened_from_overlay = false;
    overlay.settings_focus_until = None;
}

fn publish(app: &AppHandle) {
    let _ = app.emit(EVT_STATE, snapshot(app));
}

pub fn sync_session(app: &AppHandle, tracker: &TrackerSnapshot, settings: &Settings) {
    let target = tracker.vn.as_ref().filter(|vn| vn.media_type == "vn").map(|vn| Target {
        pid: vn.pid,
        exe: vn.exe.clone(),
        content_id: vn.content_id.clone(),
        title: vn.media_title.clone(),
        started_at: vn.started_at.clone(),
    });
    let automatic = target.as_ref().is_some_and(|target| {
        settings.vn_open_texthooker && app.state::<AppState>().db.list_vn_apps().unwrap_or_default()
            .iter().any(|entry| entry.exe == target.exe && entry.reader_mode == ReaderMode::Overlay)
    });
    let mut overlay = runtime(app).lock().unwrap();
    let changed = overlay.set_session(target, automatic);
    drop(overlay);
    if changed {
        if let Some(window) = app.get_webview_window(LABEL) { let _ = window.set_ignore_cursor_events(false); }
        if let Some(window) = app.get_webview_window(POPUP_LABEL) { let _ = window.hide(); }
        publish(app);
    }
}

pub fn accept_line(app: &AppHandle, line: &TextLine) {
    let mut overlay = runtime(app).lock().unwrap();
    if !overlay.view.target.as_ref().is_some_and(|target| accepts_line(target, line)) { return; }
    overlay.view.line = Some(line.clone());
    drop(overlay);
    publish(app);
}

fn accepts_line(target: &Target, line: &TextLine) -> bool {
    target.content_id == line.content_id && line.created_at >= target.started_at
}

pub fn spawn(app: AppHandle) {
    let pointer_app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            // The two transparent webviews are only built the first time the
            // overlay is wanted for a detected novel. Until then this loop only
            // looks the window up, and anime-only users never pay for them.
            let wanted = {
                let overlay = runtime(&app).lock().unwrap();
                overlay.desired && overlay.view.target.is_some()
            };
            if wanted && app.get_webview_window(POPUP_LABEL).is_none() {
                let always_on_top = app.state::<AppState>().settings_snapshot().await.reading_overlay.always_on_top;
                if let Err(error) = build_windows(&app, always_on_top) {
                    log::error!("Could not create reading overlay: {error}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }
            refresh_visibility(&app);
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });
    tauri::async_runtime::spawn(async move {
        loop {
            let active = {
                let overlay = runtime(&pointer_app).lock().unwrap();
                overlay.view.visible && (overlay.view.click_through || overlay.ignored_cursor)
            };
            if active { let _ = refresh_cursor_interaction(&pointer_app); }
            tokio::time::sleep(Duration::from_millis(if active { 16 } else { 150 })).await;
        }
    });
}

/// Builds whichever of the overlay and its popup is missing; once built they live
/// until exit, because the settings windows are owned by the popup and would be
/// destroyed with it.
fn build_windows(app: &AppHandle, always_on_top: bool) -> tauri::Result<()> {
    for (label, width, height) in [(LABEL, 800.0, 180.0), (POPUP_LABEL, 560.0, 560.0)] {
        if app.get_webview_window(label).is_some() { continue; }
        let builder = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::default())
            .title("NihongoTracker")
            .inner_size(width, height)
            .min_inner_size(320.0, if label == LABEL { 100.0 } else { 280.0 })
            .resizable(true)
            .decorations(false)
            .shadow(false)
            .transparent(true)
            .always_on_top(always_on_top)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(false);
        let builder = if label == POPUP_LABEL {
            builder.parent(&app.get_webview_window(LABEL).expect("Overlay must be created before its popup"))?
        } else { builder };
        builder.build()?;
    }
    Ok(())
}

fn refresh_visibility(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else { return; };
    let foreground = crate::platform::foreground_window();
    let hide_when_unfocused = app.state::<AppState>().settings.try_read()
        .map(|settings| settings.reading_overlay.hide_when_unfocused).unwrap_or(true);
    let settings_focused = app.get_webview_window(SETTINGS_LABEL)
        .is_some_and(|settings| settings.is_focused().ok() == Some(true));
    // Window queries wait for the event loop, whose callbacks also lock the overlay.
    let reader_focused = app.get_webview_window("texthooker")
        .is_some_and(|reader| reader.is_focused().ok() == Some(true));
    let reader_content_id = if reader_focused {
        app.state::<AppState>().hooker.try_read().ok().and_then(|reader| reader.content_id.clone())
    } else { None };
    let mut overlay = runtime(app).lock().unwrap();
    // Without focus information (Wayland) the overlay cannot follow the game,
    // so it behaves as if the game always had focus.
    let focus_known = crate::platform::capabilities().foreground;
    let game_focused = !focus_known || foreground.as_ref().is_some_and(|fg| {
        overlay.view.target.as_ref().is_some_and(|target| fg.pid == target.pid)
            && crate::platform::game_window_center(fg.pid).is_some()
    });
    let focus_pending = overlay.settings_focus_until.is_some_and(|until| Instant::now() < until);
    if settings_focused || !focus_pending { overlay.settings_focus_until = None; }
    let reader_focused = overlay.view.target.as_ref().is_some_and(|target|
        reader_content_id.as_deref() == Some(target.content_id.as_str()));
    if overlay.manual_show {
        if !game_focused { overlay.manual_seen_unfocused = true; }
        else if overlay.manual_seen_unfocused { overlay.manual_show = false; }
    }
    let visible = overlay.should_show(foreground.as_ref().filter(|_| game_focused).map(|fg| fg.pid), settings_focused || focus_pending, reader_focused, hide_when_unfocused);
    let place = if visible && !overlay.placed { overlay.view.target.as_ref().map(|target| target.pid) } else { None };
    let reveal_changed = overlay.refresh_reveal(Instant::now());
    let changed = visible != overlay.view.visible || (!visible && overlay.view.popup.is_some()) || reveal_changed;
    overlay.view.visible = visible;
    if !visible { overlay.view.popup = None; }
    let show_popup = visible && overlay.popup_ready && overlay.view.popup.is_some();
    drop(overlay);
    if let Some(pid) = place {
        place_overlay(app, &window, pid);
        runtime(app).lock().unwrap().placed = true;
    }
    if window.is_visible().ok() != Some(visible) {
        if visible { let _ = window.show(); } else { let _ = window.hide(); }
    }
    if let Some(popup) = app.get_webview_window(POPUP_LABEL) {
        if popup.is_visible().ok() != Some(show_popup) {
            if show_popup { let _ = popup.show(); } else { let _ = popup.hide(); }
        }
    }
    if changed { publish(app); }
}

fn place_overlay(app: &AppHandle, window: &tauri::WebviewWindow, pid: u32) {
    let bounds = crate::platform::game_window_center(pid);
    let monitor = bounds.and_then(|(x, y)| window.monitor_from_point(x, y).ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return; };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let saved = app.state::<AppState>().db.get_kv("reading-overlay:geometry")
        .and_then(|value| serde_json::from_str::<Geometry>(&value).ok());
    let size = saved.as_ref().map(|g| ((g.width as f64 * scale) as u32, (g.height as f64 * scale) as u32))
        .unwrap_or(((800.0 * scale) as u32, (180.0 * scale) as u32));
    let width = size.0.clamp((320.0 * scale) as u32, area.size.width.max((320.0 * scale) as u32));
    let height = size.1.clamp((100.0 * scale) as u32, area.size.height.max((100.0 * scale) as u32));
    let x = saved.as_ref().map(|g| g.x).unwrap_or(area.position.x + (area.size.width as i32 - width as i32) / 2);
    let y = saved.as_ref().map(|g| g.y).unwrap_or(area.position.y + area.size.height as i32 - height as i32 - (60.0 * scale) as i32);
    let x = x.clamp(area.position.x, area.position.x + area.size.width.saturating_sub(width) as i32);
    let y = y.clamp(area.position.y, area.position.y + area.size.height.saturating_sub(height) as i32);
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.set_size(PhysicalSize::new(width, height));
}

pub fn save_geometry(app: &AppHandle, label: &str) {
    if !matches!(label, LABEL | POPUP_LABEL) { return; }
    if label == LABEL && !runtime(app).lock().unwrap().placed { return; }
    if label == POPUP_LABEL && !runtime(app).lock().unwrap().popup_placed { return; }
    let Some(window) = app.get_webview_window(label) else { return; };
    if let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) {
        let scale = window.scale_factor().unwrap_or(1.0);
        let geometry = Geometry { x: position.x, y: position.y, width: (size.width as f64 / scale).round() as u32, height: (size.height as f64 / scale).round() as u32 };
        if let Ok(value) = serde_json::to_string(&geometry) {
            let _ = app.state::<AppState>().db.set_kv(&format!("{label}:geometry"), &value);
        }
    }
}

pub fn ready(app: &AppHandle, popup: bool) {
    let mut overlay = runtime(app).lock().unwrap();
    if popup { overlay.popup_ready = true; } else { overlay.ready = true; }
    let show_popup = popup && overlay.view.visible && overlay.view.popup.is_some();
    drop(overlay);
    if show_popup {
        if let Some(window) = app.get_webview_window(POPUP_LABEL) { let _ = window.show(); }
    }
}

pub fn toggle(app: &AppHandle) {
    let mut overlay = runtime(app).lock().unwrap();
    if overlay.view.target.is_none() { return; }
    overlay.desired = !overlay.desired;
    overlay.manual_show = overlay.desired;
    overlay.manual_seen_unfocused = false;
    if overlay.desired { overlay.reveal(); }
    drop(overlay);
    refresh_visibility(app);
}

pub fn hide(app: &AppHandle) {
    let mut overlay = runtime(app).lock().unwrap();
    overlay.desired = false;
    overlay.manual_show = false;
    overlay.reveal_until = None;
    drop(overlay);
    refresh_visibility(app);
}

pub fn show(app: &AppHandle) {
    let mut overlay = runtime(app).lock().unwrap();
    if overlay.view.target.is_none() { return; }
    overlay.desired = true;
    overlay.manual_show = true;
    overlay.manual_seen_unfocused = false;
    overlay.reveal();
    drop(overlay);
    refresh_visibility(app);
}

pub fn toggle_interaction(app: &AppHandle) -> Result<(), String> {
    let next = !snapshot(app).click_through;
    runtime(app).lock().unwrap().view.click_through = next;
    refresh_cursor_interaction(app)?;
    publish(app);
    Ok(())
}

fn cursor_over_toolbar(cursor: PhysicalPosition<f64>, origin: PhysicalPosition<i32>, size: PhysicalSize<u32>, scale: f64) -> bool {
    let x = cursor.x - origin.x as f64;
    let y = cursor.y - origin.y as f64;
    x >= 0.0 && x < size.width as f64 && y >= 0.0 && y < (32.0 * scale).min(size.height as f64)
}

fn refresh_cursor_interaction(app: &AppHandle) -> Result<(), String> {
    let window = app.get_webview_window(LABEL).ok_or("Overlay unavailable")?;
    let view = snapshot(app);
    let toolbar_hovered = view.click_through && view.visible && cursor_over_toolbar(
        window.cursor_position().map_err(|e| e.to_string())?,
        window.inner_position().map_err(|e| e.to_string())?,
        window.inner_size().map_err(|e| e.to_string())?,
        window.scale_factor().map_err(|e| e.to_string())?,
    );
    let ignore = view.click_through && !toolbar_hovered;
    let mut overlay = runtime(app).lock().unwrap();
    if overlay.view.click_through != view.click_through { return Ok(()); }
    if overlay.ignored_cursor != ignore {
        drop(overlay);
        window.set_ignore_cursor_events(ignore).map_err(|e| e.to_string())?;
        overlay = runtime(app).lock().unwrap();
        overlay.ignored_cursor = ignore;
    }
    let changed = overlay.view.toolbar_hovered != toolbar_hovered;
    overlay.view.toolbar_hovered = toolbar_hovered;
    drop(overlay);
    if changed { publish(app); }
    Ok(())
}

pub fn restore_interaction(app: &AppHandle) -> Result<(), String> {
    if snapshot(app).click_through { toggle_interaction(app)?; }
    Ok(())
}

pub fn close_popup(app: &AppHandle) {
    runtime(app).lock().unwrap().view.popup = None;
    if let Some(window) = app.get_webview_window(POPUP_LABEL) { let _ = window.hide(); }
    publish(app);
}

#[derive(Deserialize)]
pub struct PopupAnchor {
    pub top: f64,
    pub bottom: f64,
}

fn dictionary_popup_position(anchor: PhysicalPosition<f64>, size: PhysicalSize<u32>,
    origin: PhysicalPosition<i32>, area: PhysicalSize<u32>, scale: f64, vertical: bool, span: (f64, f64)) -> PhysicalPosition<i32> {
    let gap = 12.0 * scale;
    let left = origin.x as f64;
    let top = origin.y as f64;
    let right = left + area.width as f64;
    let bottom = top + area.height as f64;
    let width = size.width as f64;
    let height = size.height as f64;
    let space_right = right - anchor.x - gap;
    let space_left = anchor.x - left - gap;
    let space_below = bottom - span.1 - gap;
    let space_above = span.0 - top - gap;
    let x = if !vertical { anchor.x } else if space_right >= width || space_right >= space_left {
        anchor.x + gap
    } else { anchor.x - gap - width };
    let y = if space_below >= height || space_below >= space_above {
        span.1 + gap
    } else { span.0 - gap - height };
    PhysicalPosition::new(x.clamp(left, (right - width).max(left)) as i32,
        y.clamp(top, (bottom - height).max(top)) as i32)
}

pub fn open_popup(app: &AppHandle, mode: &str, line_id: Option<&str>, offset: usize, x: f64, y: f64, vertical: bool, anchor: Option<PopupAnchor>) -> Result<(), String> {
    if !matches!(mode, "dictionary" | "hooks") { return Err("Invalid overlay popup".into()); }
    let view = snapshot(app);
    if !view.visible || (view.click_through && mode == "dictionary") { return Err("Overlay is not interactive".into()); }
    let target = view.target.ok_or("No active VN")?;
    let line = if mode == "dictionary" {
        let line = app.state::<AppState>().db.line_by_id(line_id.ok_or("Missing line")?)
            .map_err(|e| e.to_string())?.ok_or("Line unavailable")?;
        if !accepts_line(&target, &line) { return Err("Line belongs to another session".into()); }
        Some(line)
    } else { None };
    let window = app.get_webview_window(POPUP_LABEL).ok_or("Popup unavailable")?;
    window.set_ignore_cursor_events(false).map_err(|e| e.to_string())?;
    let parent = app.get_webview_window(LABEL).ok_or("Overlay unavailable")?;
    let scale = parent.scale_factor().map_err(|e| e.to_string())?;
    let position = parent.outer_position().map_err(|e| e.to_string())?;
    let anchor_x = position.x as f64 + x * scale;
    let anchor_y = position.y as f64 + y * scale;
    let span = anchor.filter(|anchor| !vertical && anchor.top <= anchor.bottom)
        .map(|anchor| (position.y as f64 + anchor.top * scale, position.y as f64 + anchor.bottom * scale))
        .unwrap_or((anchor_y, anchor_y));
    let monitor = window.monitor_from_point(anchor_x, anchor_y).map_err(|e| e.to_string())?
        .or_else(|| window.primary_monitor().ok().flatten()).ok_or("Monitor unavailable")?;
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let saved = app.state::<AppState>().db.get_kv("reading-overlay-popup:geometry")
        .and_then(|value| serde_json::from_str::<Geometry>(&value).ok());
    let width = saved.as_ref().map(|g| (g.width as f64 * scale) as u32).unwrap_or((560.0 * scale) as u32).min(area.size.width);
    let mut height = saved.as_ref().map(|g| (g.height as f64 * scale) as u32).unwrap_or((560.0 * scale) as u32).min(area.size.height);
    if mode == "dictionary" && !vertical {
        let gap = 12.0 * scale;
        let below = area.position.y as f64 + area.size.height as f64 - span.1 - gap;
        let above = span.0 - area.position.y as f64 - gap;
        height = height.min(below.max(above).max(1.0) as u32);
    }
    let opening = view.popup.as_ref().map(|popup| popup.mode.as_str()) != Some(mode)
        || window.is_visible().ok() != Some(true);
    if opening || mode == "dictionary" {
        runtime(app).lock().unwrap().popup_placed = false;
        let position = if mode == "dictionary" {
            dictionary_popup_position(PhysicalPosition::new(anchor_x, anchor_y),
                PhysicalSize::new(width, height), area.position, area.size, scale, vertical, span)
        } else {
            PhysicalPosition::new(
                (anchor_x as i32 + (12.0 * scale) as i32).clamp(area.position.x, area.position.x + area.size.width.saturating_sub(width) as i32),
                (anchor_y as i32 - height as i32).clamp(area.position.y, area.position.y + area.size.height.saturating_sub(height) as i32))
        };
        window.set_position(position).map_err(|e| e.to_string())?;
        if opening || mode == "dictionary" { window.set_size(PhysicalSize::new(width, height)).map_err(|e| e.to_string())?; }
    }
    let mut overlay = runtime(app).lock().unwrap();
    overlay.popup_placed = true;
    if overlay.view.target.as_ref() != Some(&target) || !overlay.view.visible || (overlay.view.click_through && mode == "dictionary") { return Err("Session changed".into()); }
    overlay.view.popup = Some(Popup { mode: mode.into(), line, offset });
    let ready = overlay.popup_ready;
    drop(overlay);
    publish(app);
    if ready { window.show().map_err(|e| e.to_string())?; }
    Ok(())
}

pub fn apply_shortcuts(app: &AppHandle, settings: &Settings) {
    #[cfg(desktop)]
    {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
        let previous = std::mem::take(&mut runtime(app).lock().unwrap().registered_shortcuts);
        for shortcut in previous { let _ = app.global_shortcut().unregister(shortcut.as_str()); }
        let mut registered = Vec::new();
        let mut errors = Vec::new();
        for (value, interaction) in [(&settings.reading_overlay.toggle_shortcut, false), (&settings.reading_overlay.interaction_shortcut, true)] {
            if value.trim().is_empty() { continue; }
            let result = value.parse::<Shortcut>().map_err(|e| e.to_string()).and_then(|shortcut| {
                if app.global_shortcut().is_registered(shortcut) { return Err("Shortcut already registered".into()); }
                app.global_shortcut().on_shortcut(shortcut, move |app, _, event| {
                    if event.state() != ShortcutState::Pressed { return; }
                    if interaction { let _ = toggle_interaction(app); } else { toggle(app); }
                }).map_err(|e| e.to_string())
            });
            match result {
                Ok(()) => registered.push(value.clone()),
                Err(_) => errors.push(value.clone()),
            }
        }
        let mut overlay = runtime(app).lock().unwrap();
        overlay.registered_shortcuts = registered;
        overlay.view.shortcut_errors = errors;
        drop(overlay);
        publish(app);
    }
}

pub fn selected_session<'a>(tracker: &'a TrackerSnapshot, target: &Target) -> Option<&'a VnSession> {
    tracker.vn.as_ref().filter(|vn| vn.pid == target.pid && vn.content_id == target.content_id && vn.started_at == target.started_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(windows, feature = "native-runtime-test"))]
    #[test]
    #[ignore = "Runs an isolated Windows event loop, run with a process timeout"]
    fn visibility_poll_and_window_callbacks_do_not_deadlock() {
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}, mpsc};
        use tokio::sync::RwLock;

        let completed = Arc::new(AtomicUsize::new(0));
        let progress = completed.clone();
        let test_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../node_modules/.cache/reading-overlay/runtime-test")
            .join(uuid::Uuid::new_v4().to_string());
        let mut context = tauri::generate_context!();
        context.config_mut().identifier = format!("app.nihongotracker.runtime-test.{}", uuid::Uuid::new_v4());
        let app = tauri::Builder::default().any_thread()
            .setup(move |app| {
                let (hooker_tx, _rx) = tokio::sync::mpsc::unbounded_channel();
                app.manage(AppState {
                    reading_overlay: Default::default(),
                    db: Arc::new(crate::db::Db::open(std::path::Path::new(":memory:"))?),
                    dictionaries: Arc::new(crate::dictionary::DictionaryStore::open(test_dir.join("dictionaries"))),
                    settings: RwLock::new(Settings::default()),
                    tracker: RwLock::new(Default::default()),
                    hooker: RwLock::new(Default::default()),
                    hooker_tx,
                    media_cache: Default::default(), playback: Default::default(),
                    playback_probing: Default::default(), hooker_timer_active: Default::default(),
                    session_paused: Default::default(), vn_timer_edit: Default::default(),
                    streak: Default::default(), streak_auth_failed: Default::default(),
                    discord: Default::default(), game_hook: Default::default(),
                    text_cache_tasks: Default::default(), connection_online: Default::default(),
                    browser_bridge: Default::default(),
                    outbox_flush: Default::default(),
                    main_intent: Default::default(),
                    connection_last: Default::default(),
                });
                for label in [LABEL, "texthooker"] {
                    tauri::WebviewWindowBuilder::new(app, label,
                        tauri::WebviewUrl::External("about:blank".parse()?))
                        .visible(false).data_directory(test_dir.join(label)).build()?;
                }
                tauri::WebviewWindowBuilder::new(app, POPUP_LABEL,
                    tauri::WebviewUrl::External("about:blank".parse()?))
                    .parent(&app.get_webview_window(LABEL).unwrap())?
                    .visible(false).data_directory(test_dir.join(POPUP_LABEL)).build()?;
                Ok(())
            })
            .build(context).unwrap();
        let exit = app.run_return(move |app, event| {
            if !matches!(event, tauri::RunEvent::Ready) { return; }
            let app = app.clone();
            let progress = progress.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    let (entered_tx, entered_rx) = mpsc::channel();
                    let callback_app = app.clone();
                    app.run_on_main_thread(move || {
                        entered_tx.send(()).unwrap();
                        // Overlap the focus query with an event callback that needs overlay state.
                        std::thread::sleep(Duration::from_millis(20));
                        save_geometry(&callback_app, LABEL);
                        let _ = snapshot(&callback_app);
                    }).unwrap();
                    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    refresh_visibility(&app);
                    progress.fetch_add(1, Ordering::SeqCst);
                }
                use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_OWNER, GW_HWNDNEXT};
                use windows::Win32::Foundation::HWND;
                let popup = HWND(app.get_webview_window(POPUP_LABEL).unwrap().hwnd().unwrap().0);
                for kind in ["settings", SETTINGS_LABEL] {
                    tauri::async_runtime::block_on(crate::commands::open_aux_window(app.clone(), kind.into())).unwrap();
                    let window = app.get_webview_window(kind).unwrap();
                    let handle = HWND(window.hwnd().unwrap().0);
                    assert_eq!(unsafe { GetWindow(handle, GW_OWNER).unwrap() }, popup);
                    window.hide().unwrap();
                    tauri::async_runtime::block_on(crate::commands::open_aux_window(app.clone(), kind.into())).unwrap();
                    assert_eq!(window.hwnd().unwrap().0, handle.0);
                    assert_eq!(unsafe { GetWindow(handle, GW_OWNER).unwrap() }, popup);
                    let popup_window = app.get_webview_window(POPUP_LABEL).unwrap();
                    popup_window.show().unwrap();
                    let mut below = handle;
                    let mut popup_below = false;
                    for _ in 0..1000 {
                        let Ok(next) = (unsafe { GetWindow(below, GW_HWNDNEXT) }) else { break; };
                        if next == popup { popup_below = true; break; }
                        if next.0.is_null() { break; }
                        below = next;
                    }
                    assert!(popup_below, "Settings must stay above a newly shown dictionary");
                    popup_window.hide().unwrap();
                    assert!(window.is_visible().unwrap());
                    window.hide().unwrap();
                }
                app.exit(0);
            });
        });
        assert_eq!(exit, 0);
        assert_eq!(completed.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn dictionary_popup_tracks_the_anchor_and_flips_at_screen_edges() {
        let origin = PhysicalPosition::new(0, 0);
        let area = PhysicalSize::new(1920, 1080);
        let size = PhysicalSize::new(560, 560);
        assert_eq!(dictionary_popup_position(PhysicalPosition::new(200.0, 100.0), size, origin, area, 1.0, true, (100.0, 100.0)), PhysicalPosition::new(212, 112));
        assert_eq!(dictionary_popup_position(PhysicalPosition::new(600.0, 120.0), size, origin, area, 1.0, true, (120.0, 120.0)), PhysicalPosition::new(612, 132));
        assert_eq!(dictionary_popup_position(PhysicalPosition::new(1800.0, 1000.0), size, origin, area, 1.0, true, (1000.0, 1000.0)), PhysicalPosition::new(1228, 428));
    }

    #[test]
    fn dictionary_popup_stays_on_a_scaled_monitor_with_negative_coordinates() {
        let origin = PhysicalPosition::new(-1920, -200);
        let area = PhysicalSize::new(1920, 1080);
        let size = PhysicalSize::new(840, 840);
        let position = dictionary_popup_position(PhysicalPosition::new(-80.0, 750.0), size, origin, area, 1.5, true, (750.0, 750.0));
        assert_eq!(position, PhysicalPosition::new(-938, -108));
        let position = dictionary_popup_position(PhysicalPosition::new(-1900.0, -180.0), area, origin, area, 1.5, true, (-180.0, -180.0));
        assert_eq!(position, origin);
    }

    #[test]
    fn horizontal_dictionary_opens_above_or_below_without_covering_the_anchor() {
        let origin = PhysicalPosition::new(-1920, -200);
        let area = PhysicalSize::new(1920, 1080);
        let size = PhysicalSize::new(560, 560);
        let position = dictionary_popup_position(PhysicalPosition::new(-1500.0, -100.0), size, origin, area, 1.0, false, (-140.0, -60.0));
        assert_eq!(position, PhysicalPosition::new(-1500, -48));
        let position = dictionary_popup_position(PhysicalPosition::new(-80.0, 750.0), size, origin, area, 1.5, false, (710.0, 790.0));
        assert_eq!(position, PhysicalPosition::new(-560, 132));
        assert!(position.y + (size.height as i32) < 710);
    }

    #[test]
    fn manual_reveal_expires_and_does_not_carry_into_another_session() {
        let mut overlay = Runtime::default();
        let target = Target { pid: 42, exe: "game.exe".into(), content_id: "v1".into(), title: "VN".into(), started_at: "start".into() };
        overlay.set_session(Some(target), true);
        overlay.reveal();
        let until = overlay.reveal_until.unwrap();
        assert!(overlay.refresh_reveal(until - Duration::from_secs(1)));
        assert!(overlay.view.reveal_controls);
        assert!(overlay.refresh_reveal(until));
        assert!(!overlay.view.reveal_controls);
        overlay.reveal();
        overlay.refresh_reveal(Instant::now());
        overlay.set_session(None, false);
        assert!(!overlay.view.reveal_controls);
        assert!(overlay.reveal_until.is_none());
    }

    #[test]
    fn manual_show_and_always_visible_allow_other_foreground_windows() {
        let mut overlay = Runtime::default();
        overlay.ready = true;
        let target = Target { pid: 42, exe: "game.exe".into(), content_id: "v1".into(), title: "VN".into(), started_at: "start".into() };
        overlay.set_session(Some(target.clone()), true);
        assert!(!overlay.should_show(Some(99), false, false, true));
        assert!(overlay.should_show(Some(99), false, false, false));
        overlay.manual_show = true;
        assert!(overlay.should_show(Some(99), false, false, true));
        overlay.set_session(Some(Target { title: "Updated title".into(), ..target }), true);
        assert!(overlay.manual_show);
        overlay.desired = false;
        assert!(!overlay.should_show(Some(99), false, false, false));
        overlay.set_session(None, false);
        assert!(!overlay.manual_show);
        assert!(!overlay.should_show(Some(99), false, false, false));
    }

    #[test]
    fn click_through_keeps_only_the_toolbar_interactive_at_each_monitor_scale() {
        let origin = PhysicalPosition::new(-800, 200);
        let size = PhysicalSize::new(800, 300);
        for scale in [1.0, 1.5, 2.0] {
            assert!(cursor_over_toolbar(PhysicalPosition::new(-400.0, 200.0 + 31.0 * scale), origin, size, scale));
            assert!(!cursor_over_toolbar(PhysicalPosition::new(-400.0, 200.0 + 32.0 * scale), origin, size, scale));
            assert!(!cursor_over_toolbar(PhysicalPosition::new(-801.0, 201.0), origin, size, scale));
            assert!(!cursor_over_toolbar(PhysicalPosition::new(0.0, 201.0), origin, size, scale));
        }
    }

    #[test]
    fn lines_must_belong_to_the_work_and_current_launch() {
        let target = Target { pid: 42, exe: "game.exe".into(), content_id: "v1".into(), title: "VN".into(), started_at: "2026-10-03T12:00:00Z".into() };
        let mut line = TextLine { id: "line".into(), content_id: "v1".into(), text: "日本語".into(), chars: 3, created_at: "2026-10-03T12:00:01Z".into(), synced: false };
        assert!(accepts_line(&target, &line));
        line.content_id = "v2".into();
        assert!(!accepts_line(&target, &line));
        line.content_id = "v1".into();
        line.created_at = "2026-10-03T11:59:59Z".into();
        assert!(!accepts_line(&target, &line));
    }

    #[test]
    fn manually_hidden_overlay_stays_hidden_until_a_new_launch() {
        let target = Target { pid: 42, exe: "game.exe".into(), content_id: "v1".into(), title: "VN".into(), started_at: "start".into() };
        let mut overlay = Runtime::default();
        overlay.ready = true;
        assert!(overlay.set_session(Some(target.clone()), true));
        assert!(overlay.should_show(Some(42), false, false, true));
        assert!(!overlay.should_show(Some(99), false, false, true));
        assert!(overlay.should_show(Some(42), false, false, true));
        overlay.desired = false;
        assert!(!overlay.set_session(Some(target.clone()), true));
        assert!(!overlay.should_show(Some(42), false, false, true));
        let next = Target { started_at: "next launch".into(), ..target };
        assert!(overlay.set_session(Some(next), true));
        assert!(overlay.should_show(Some(42), false, false, true));
        overlay.set_session(None, false);
        assert!(!overlay.should_show(Some(42), false, false, true));
    }

    #[test]
    fn overlay_settings_keep_only_the_active_overlay_visible() {
        let target = Target { pid: 42, exe: "game.exe".into(), content_id: "v1".into(), title: "VN".into(), started_at: "start".into() };
        let mut overlay = Runtime::default();
        overlay.ready = true;
        overlay.set_session(Some(target), true);
        overlay.settings_opened_from_overlay = true;
        assert!(overlay.should_show(None, true, false, true));
        assert!(!overlay.should_show(None, false, false, true));
        overlay.settings_opened_from_overlay = false;
        assert!(!overlay.should_show(None, true, false, true));
    }

    #[test]
    fn reader_focus_keeps_the_overlay_visible_for_the_current_session() {
        let mut overlay = Runtime::default();
        overlay.ready = true;
        overlay.set_session(Some(Target {
            pid: 42, exe: "game.exe".into(), content_id: "v1".into(),
            title: "VN".into(), started_at: "start".into(),
        }), true);
        assert!(overlay.should_show(None, false, true, true));
        assert!(!overlay.should_show(None, false, false, true));
        overlay.desired = false;
        assert!(!overlay.should_show(None, false, true, true));
        overlay.set_session(None, false);
        assert!(!overlay.should_show(None, false, true, true));
    }

    #[test]
    fn session_change_drops_popup_line_and_click_through() {
        let target = Target { pid: 42, exe: "game.exe".into(), content_id: "v1".into(), title: "VN".into(), started_at: "start".into() };
        let line = TextLine { id: "line".into(), content_id: "v1".into(), text: "日本語".into(), chars: 3, created_at: "start".into(), synced: false };
        let mut overlay = Runtime::default();
        overlay.set_session(Some(target.clone()), true);
        overlay.view.line = Some(line.clone());
        overlay.view.popup = Some(Popup { mode: "dictionary".into(), line: Some(line), offset: 0 });
        overlay.view.click_through = true;
        overlay.set_session(Some(Target { pid: 43, ..target }), true);
        assert!(overlay.view.line.is_none());
        assert!(overlay.view.popup.is_none());
        assert!(!overlay.view.click_through);
    }
}
