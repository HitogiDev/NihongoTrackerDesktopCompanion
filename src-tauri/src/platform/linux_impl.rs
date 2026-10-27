//! Linux: X11 through `x11rb`, which is pure Rust and needs no system library.
//!
//! The EWMH properties every X11 window manager keeps up to date are enough:
//! `_NET_CLIENT_LIST` for the windows, `_NET_WM_PID` and `_NET_WM_NAME` for
//! each one, `_NET_ACTIVE_WINDOW` for focus. Idle time comes from the
//! MIT-SCREEN-SAVER extension.
//!
//! Under Wayland the X server is XWayland and only knows the X11 clients (Wine
//! and Proton games, most of the time, but rarely a native player). Players are
//! covered by MPRIS there, and idle time is asked to the compositor over D-Bus.

use std::sync::Mutex;

use once_cell::sync::Lazy;
use x11rb::connection::{Connection, RequestConnection as _};
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, ImageFormat, Window};
use x11rb::rust_connection::RustConnection;

use super::{Capabilities, WindowInfo};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_CLIENT_LIST,
        _NET_ACTIVE_WINDOW,
        _NET_WM_PID,
        _NET_WM_NAME,
        UTF8_STRING,
    }
}

struct X11 {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
    screensaver: bool,
}

/// One connection for the life of the app. `None` when there is no X server
/// (a pure Wayland session without XWayland, or a headless machine).
static X11_CONN: Lazy<Mutex<Option<X11>>> = Lazy::new(|| Mutex::new(connect()));

fn connect() -> Option<X11> {
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    let atoms = Atoms::new(&conn).ok()?.reply().ok()?;
    let screensaver = conn
        .extension_information(x11rb::protocol::screensaver::X11_EXTENSION_NAME)
        .ok()
        .flatten()
        .is_some();
    Some(X11 { conn, root, atoms, screensaver })
}

fn with_x11<T>(f: impl FnOnce(&X11) -> Option<T>) -> Option<T> {
    let guard = X11_CONN.lock().ok()?;
    f(guard.as_ref()?)
}

fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind.eq_ignore_ascii_case("wayland"))
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Cheap enough to call on every tick: no round trip to the X server or the bus.
pub fn capabilities() -> Capabilities {
    let wayland = is_wayland();
    let (x11, screensaver) = with_x11(|x| Some((true, x.screensaver))).unwrap_or((false, false));
    Capabilities {
        os: "linux".into(),
        session: Some(if wayland { "wayland" } else { "x11" }.into()),
        // Under Wayland the X server only sees XWayland clients, so neither
        // titles nor focus can be trusted for the whole desktop.
        window_titles: x11 && !wayland,
        foreground: x11 && !wayland,
        idle: if wayland { super::dbus::compositor_idle_available() } else { screensaver },
        media_sessions: super::dbus::available(),
        native_hook: false,
        screenshots: x11 && !wayland,
        actionable_notifications: true,
        needs_screen_recording: false,
    }
}

fn window_pid(x: &X11, window: Window) -> Option<u32> {
    let reply = x.conn
        .get_property(false, window, x.atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    let pid = reply.value32()?.next()?;
    (pid != 0).then_some(pid)
}

fn window_title(x: &X11, window: Window) -> String {
    let utf8 = x.conn
        .get_property(false, window, x.atoms._NET_WM_NAME, x.atoms.UTF8_STRING, 0, 1024)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .filter(|reply| !reply.value.is_empty());
    if let Some(reply) = utf8 {
        return String::from_utf8_lossy(&reply.value).into_owned();
    }
    // Older toolkits only set the ICCCM name, in Latin-1.
    x.conn
        .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.value.iter().map(|&byte| byte as char).collect())
        .unwrap_or_default()
}

fn info(x: &X11, window: Window) -> Option<WindowInfo> {
    let pid = window_pid(x, window)?;
    Some(WindowInfo { pid, title: window_title(x, window), process: String::new() })
}

fn active_window(x: &X11) -> Option<Window> {
    let reply = x.conn
        .get_property(false, x.root, x.atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    let window = reply.value32()?.next();
    window.filter(|&window| window != 0)
}

/// Every managed, titled top-level window.
pub fn visible_windows() -> Vec<WindowInfo> {
    with_x11(|x| {
        let reply = x.conn
            .get_property(false, x.root, x.atoms._NET_CLIENT_LIST, AtomEnum::WINDOW, 0, u32::MAX)
            .ok()?
            .reply()
            .ok()?;
        let windows: Vec<Window> = reply.value32()?.collect();
        // A window can close between the list and the query; it is skipped.
        Some(
            windows.into_iter()
                .filter_map(|window| info(x, window))
                .filter(|window| !window.title.trim().is_empty())
                .collect(),
        )
    })
    .unwrap_or_default()
}

pub fn foreground_window() -> Option<WindowInfo> {
    with_x11(|x| info(x, active_window(x)?))
}

pub fn game_window_center(pid: u32) -> Option<(f64, f64)> {
    with_x11(|x| {
        let window = active_window(x)?;
        if window_pid(x, window)? != pid {
            return None;
        }
        let geometry = x.conn.get_geometry(window).ok()?.reply().ok()?;
        let origin = x.conn.translate_coordinates(window, x.root, 0, 0).ok()?.reply().ok()?;
        Some((
            origin.dst_x as f64 + geometry.width as f64 / 2.0,
            origin.dst_y as f64 + geometry.height as f64 / 2.0,
        ))
    })
}

/// Screenshot of the focused window, when it belongs to `pid`. Done with a
/// plain `GetImage` instead of `xcap`, whose Linux backend links PipeWire and
/// libxcb — two more system libraries for every Linux build.
pub fn capture_window(pid: u32, _title: &str, path: &std::path::Path) -> bool {
    let image = with_x11(|x| {
        let window = active_window(x)?;
        if window_pid(x, window)? != pid {
            return None;
        }
        let geometry = x.conn.get_geometry(window).ok()?.reply().ok()?;
        // 24/32-bit TrueColor, the only depth a game window has in practice:
        // 4 bytes per pixel, BGRX in memory.
        if geometry.depth < 24 {
            return None;
        }
        let (width, height) = (geometry.width as u32, geometry.height as u32);
        let reply = x.conn
            .get_image(ImageFormat::Z_PIXMAP, window, 0, 0, geometry.width, geometry.height, u32::MAX)
            .ok()?
            .reply()
            .ok()?;
        let pixels: Vec<u8> = reply.data.chunks_exact(4).flat_map(|px| [px[2], px[1], px[0], 255]).collect();
        image::RgbaImage::from_raw(width, height, pixels)
    });
    image.is_some_and(|image| image.save(path).is_ok())
}

/// Milliseconds since the last input, or `None` when nobody will say.
fn idle_millis() -> Option<u64> {
    if is_wayland() {
        // XWayland's screensaver extension only sees input sent to X11
        // clients, so it would report "idle" while the user types elsewhere.
        return super::dbus::compositor_idle_millis();
    }
    with_x11(|x| {
        if !x.screensaver {
            return None;
        }
        let reply = x.conn.screensaver_query_info(x.root).ok()?.reply().ok()?;
        Some(reply.ms_since_user_input as u64)
    })
}

pub fn idle_seconds() -> u64 {
    idle_millis().unwrap_or(0) / 1000
}
