//! Platform abstraction for window / process inspection.
//!
//! Each OS answers the same questions — which windows exist and what they are
//! titled, which one has focus, how long the user has been idle — with what it
//! actually offers:
//!
//! * **Windows** — Win32 (`windows_impl.rs`). Everything is available.
//! * **Linux** — X11 through `x11rb` (`linux_impl.rs`), plus MPRIS over D-Bus
//!   for media players (`dbus.rs`). A Wayland session only exposes XWayland
//!   windows, so players are read from MPRIS there, and idle time comes from the
//!   compositor's D-Bus interface.
//! * **macOS** — CoreGraphics (`macos_impl.rs`). Window titles need the Screen
//!   Recording permission; without it only processes and focus are known.
//!
//! What works where is reported by [`capabilities`], so the UI can say exactly
//! what is missing instead of "unsupported".

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WindowInfo {
    pub pid: u32,
    pub title: String,
    /// Executable name, lowercased (e.g. `mpv.exe`). Filled in by the caller.
    pub process: String,
}

/// A player that publishes its state on the desktop's media bus (MPRIS on
/// Linux). Detection reads these when window titles are not available.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlayer {
    /// The player's MPRIS name without the `org.mpris.MediaPlayer2.` prefix or
    /// instance suffix, lowercased (`mpv`, `vlc`, `celluloid`).
    pub name: String,
    pub title: Option<String>,
    /// `xesam:url`, usually `file:///…/[Group] Show - 03.mkv`.
    pub url: Option<String>,
    pub position_secs: Option<f64>,
    pub duration_secs: Option<f64>,
    pub paused: bool,
}

/// What this platform can observe right now. Mirrored in `src/lib/types.ts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Capabilities {
    /// `windows`, `linux` or `macos`.
    pub os: String,
    /// `x11` or `wayland` on Linux.
    pub session: Option<String>,
    /// Window titles can be read: players are detected by the file in their title.
    pub window_titles: bool,
    /// The focused window is known: focus-only counting, game time, overlay.
    pub foreground: bool,
    /// Time since the last input is known: the AFK timeout works.
    pub idle: bool,
    /// Players publish what they play on a media bus (MPRIS).
    pub media_sessions: bool,
    /// LunaHook injection into the game process.
    pub native_hook: bool,
    /// Mined sentences can carry a screenshot of the game window.
    pub screenshots: bool,
    /// The streak reminder can carry a "log now" button.
    pub actionable_notifications: bool,
    /// macOS only: titles are hidden until the Screen Recording permission is
    /// granted; the settings screen offers to request it.
    pub needs_screen_recording: bool,
    /// Ctrl can be kept from the game while the cursor is over the reading
    /// overlay (a novel skips text while Ctrl is held).
    pub ctrl_shield: bool,
}

/// Where a Ctrl key is in the shield: what the game saw of its last press.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const CTRL_UP: u8 = 0;
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const CTRL_SWALLOWED: u8 = 1;
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const CTRL_PASSED: u8 = 2;

/// One Ctrl event through the shield: the key's new state and whether to
/// swallow it. A press and its release always go the same way, so the game
/// never sees a release without its press, nor keeps a Ctrl held that it will
/// never see released (which would leave the novel skipping).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn ctrl_event(state: u8, up: bool, blocking: bool) -> (u8, bool) {
    match (up, state) {
        (true, CTRL_SWALLOWED) => (CTRL_UP, true),
        (true, _) => (CTRL_UP, false),
        // Auto-repeat follows the first press, wherever the cursor is now.
        (false, CTRL_SWALLOWED) => (CTRL_SWALLOWED, true),
        (false, CTRL_PASSED) => (CTRL_PASSED, false),
        (false, _) if blocking => (CTRL_SWALLOWED, true),
        (false, _) => (CTRL_PASSED, false),
    }
}

#[cfg(windows)]
mod windows_impl;
#[cfg(windows)]
use windows_impl as imp;

#[cfg(target_os = "linux")]
mod linux_impl;
#[cfg(target_os = "linux")]
mod dbus;
#[cfg(target_os = "linux")]
use linux_impl as imp;

#[cfg(target_os = "macos")]
mod macos_impl;
#[cfg(target_os = "macos")]
use macos_impl as imp;

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod stub;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
use stub as imp;

/// `shield_ctrl(armed, blocking)`: while `armed` a keyboard hook is installed
/// (and removed when disarmed); while `blocking` new Ctrl presses do not reach
/// any application. A no-op where `Capabilities::ctrl_shield` is false.
pub use imp::{capabilities, foreground_window, game_window_center, idle_seconds, shield_ctrl, visible_windows};

/// Saves a PNG of `pid`'s window titled `title` (the focused game, for a mined
/// sentence). False when the platform cannot capture it.
#[cfg(any(windows, target_os = "macos"))]
pub fn capture_window(pid: u32, title: &str, path: &std::path::Path) -> bool {
    let window = xcap::Window::all().ok().and_then(|windows| {
        windows.into_iter().find(|window| window.pid().ok() == Some(pid) && window.title().ok().as_deref() == Some(title))
    });
    window
        .and_then(|window| window.capture_image().ok())
        .is_some_and(|image| image.save(path).is_ok())
}
#[cfg(target_os = "linux")]
pub use linux_impl::capture_window;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn capture_window(_pid: u32, _title: &str, _path: &std::path::Path) -> bool {
    false
}

/// Players on the media bus. Empty where there is no such bus.
pub fn media_players() -> Vec<MediaPlayer> {
    #[cfg(target_os = "linux")]
    return dbus::players();
    #[cfg(not(target_os = "linux"))]
    Vec::new()
}

/// Asks for whatever permission window titles need. Only macOS has one; the
/// system shows its own prompt the first time and the user grants it in
/// System Settings → Privacy & Security → Screen Recording.
pub fn request_window_titles_permission() -> bool {
    #[cfg(target_os = "macos")]
    return macos_impl::request_screen_recording();
    #[cfg(not(target_os = "macos"))]
    capabilities().window_titles
}

/// True when detection can find anything at all on this platform.
pub fn supported() -> bool {
    let caps = capabilities();
    caps.window_titles || caps.media_sessions || caps.foreground
}

/// What a process is called in the library and the player table.
///
/// On Windows that is the image name. Elsewhere `name()` is the kernel's
/// 15-character `comm` on Linux, so the executable's file name is preferred,
/// and for Wine / Proton the Windows executable in the command line, which is
/// what the user registered and what the window belongs to.
pub fn process_key(process: &sysinfo::Process) -> String {
    #[cfg(not(windows))]
    {
        if let Some(windows_exe) = process.cmd().first().and_then(|arg| wine_executable(&arg.to_string_lossy())) {
            return windows_exe;
        }
        if let Some(name) = process.exe().and_then(|path| path.file_name()) {
            return name.to_string_lossy().to_ascii_lowercase();
        }
    }
    process.name().to_string_lossy().to_ascii_lowercase()
}

/// `Z:\games\Novel\Novel.exe` or `/home/u/Novel/Novel.exe` → `novel.exe`.
#[cfg_attr(windows, allow(dead_code))]
fn wine_executable(arg: &str) -> Option<String> {
    let base = arg.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    base.ends_with(".exe").then_some(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_presses_and_releases_go_the_same_way() {
        // Pressed over the overlay: swallowed, repeats too, and its release
        // even after the cursor left.
        let (state, swallow) = ctrl_event(CTRL_UP, false, true);
        assert!(swallow);
        assert_eq!(ctrl_event(state, false, false), (CTRL_SWALLOWED, true));
        assert_eq!(ctrl_event(state, true, false), (CTRL_UP, true));
        // Pressed elsewhere: the game keeps it, and gets its release over the overlay.
        let (state, swallow) = ctrl_event(CTRL_UP, false, false);
        assert!(!swallow);
        assert_eq!(ctrl_event(state, false, true), (CTRL_PASSED, false));
        assert_eq!(ctrl_event(state, true, true), (CTRL_UP, false));
        // A stray release passes untouched.
        assert_eq!(ctrl_event(CTRL_UP, true, true), (CTRL_UP, false));
    }

    #[test]
    fn wine_command_lines_resolve_to_the_windows_executable() {
        assert_eq!(wine_executable(r"Z:\games\Sono Hanabira\SonoHana.exe").as_deref(), Some("sonohana.exe"));
        assert_eq!(wine_executable("/home/u/.wine/drive_c/VN/Game.EXE").as_deref(), Some("game.exe"));
        assert_eq!(wine_executable("/usr/bin/mpv"), None);
        assert_eq!(wine_executable(""), None);
    }
}
