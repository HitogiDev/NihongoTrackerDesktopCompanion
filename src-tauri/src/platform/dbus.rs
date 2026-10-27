//! Linux session bus: MPRIS media players and the compositor's idle time.
//!
//! MPRIS is how a Linux desktop shows "now playing" in its media controls, so
//! every common player speaks it (VLC, Celluloid, Haruna, SMPlayer; mpv through
//! the `mpv-mpris` plugin). It gives the file, the real position and the pause
//! state with no configuration, on X11 and Wayland alike.
//!
//! D-Bus calls block, so players are read by a thread of their own once a
//! second; the watcher only copies the last answer and never waits on the bus.

use std::collections::HashMap;
use std::sync::{Mutex, Once};
use std::time::Duration;

use once_cell::sync::Lazy;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

use super::MediaPlayer;

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";

static SESSION: Lazy<Option<Connection>> = Lazy::new(|| Connection::session().ok());
static PLAYERS: Lazy<Mutex<Vec<MediaPlayer>>> = Lazy::new(Default::default);
static POLLER: Once = Once::new();

pub fn available() -> bool {
    SESSION.is_some()
}

/// The players seen on the last poll. Starts the poller on first use.
pub fn players() -> Vec<MediaPlayer> {
    let Some(conn) = SESSION.as_ref() else { return Vec::new() };
    POLLER.call_once(|| {
        let conn = conn.clone();
        let _ = std::thread::Builder::new().name("mpris".into()).spawn(move || loop {
            let found = poll(&conn);
            if let Ok(mut players) = PLAYERS.lock() {
                *players = found;
            }
            std::thread::sleep(Duration::from_secs(1));
        });
    });
    PLAYERS.lock().map(|players| players.clone()).unwrap_or_default()
}

fn poll(conn: &Connection) -> Vec<MediaPlayer> {
    let names: Vec<String> = conn
        .call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "ListNames", &())
        .ok()
        .and_then(|reply| reply.body().deserialize().ok())
        .unwrap_or_default();
    names.iter()
        .filter_map(|bus| bus.strip_prefix(MPRIS_PREFIX).map(|rest| (bus, rest)))
        .filter_map(|(bus, rest)| read_player(conn, bus, &player_name(rest)))
        .collect()
}

/// `mpv.instance1234` → `mpv`, `io.github.celluloid_player.Celluloid` → `celluloid`.
fn player_name(rest: &str) -> String {
    rest.split('.')
        .filter(|part| !part.starts_with("instance"))
        .last()
        .unwrap_or(rest)
        .to_ascii_lowercase()
}

fn read_player(conn: &Connection, bus: &str, name: &str) -> Option<MediaPlayer> {
    let props: HashMap<String, OwnedValue> = conn
        .call_method(Some(bus), MPRIS_PATH, Some("org.freedesktop.DBus.Properties"), "GetAll", &("org.mpris.MediaPlayer2.Player",))
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    let status = props.get("PlaybackStatus").and_then(as_str)?;
    if status == "Stopped" {
        return None;
    }
    let metadata: HashMap<String, OwnedValue> = props
        .get("Metadata")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| HashMap::try_from(value).ok())
        .unwrap_or_default();
    Some(MediaPlayer {
        name: name.to_string(),
        title: metadata.get("xesam:title").and_then(as_str).map(str::to_string),
        url: metadata.get("xesam:url").and_then(as_str).map(str::to_string),
        // Both are microseconds; some players send the length unsigned.
        position_secs: props.get("Position").and_then(as_micros).map(|us| us as f64 / 1e6),
        duration_secs: metadata.get("mpris:length").and_then(as_micros).map(|us| us as f64 / 1e6),
        paused: status == "Paused",
    })
}

fn as_str(value: &OwnedValue) -> Option<&str> {
    match &**value {
        Value::Str(text) => Some(text.as_str()),
        _ => None,
    }
}

fn as_micros(value: &OwnedValue) -> Option<i64> {
    match &**value {
        Value::I64(n) => Some(*n),
        Value::U64(n) => i64::try_from(*n).ok(),
        Value::I32(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        _ => None,
    }
}

/// Whether the compositor answers at all, asked once.
pub fn compositor_idle_available() -> bool {
    static AVAILABLE: Lazy<bool> = Lazy::new(|| compositor_idle_millis().is_some());
    *AVAILABLE
}

/// Idle time from a Wayland compositor: GNOME's Mutter first, then the
/// freedesktop screensaver interface that KDE implements (in seconds).
pub fn compositor_idle_millis() -> Option<u64> {
    let conn = SESSION.as_ref()?;
    let mutter = conn
        .call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize::<u64>().ok());
    if mutter.is_some() {
        return mutter;
    }
    conn.call_method(Some("org.freedesktop.ScreenSaver"), "/ScreenSaver", Some("org.freedesktop.ScreenSaver"), "GetSessionIdleTime", &())
        .ok()
        .and_then(|reply| reply.body().deserialize::<u32>().ok())
        .map(|seconds| seconds as u64 * 1000)
}

#[cfg(test)]
mod tests {
    use super::player_name;

    #[test]
    fn mpris_bus_names_map_to_player_names() {
        assert_eq!(player_name("mpv"), "mpv");
        assert_eq!(player_name("mpv.instance4321"), "mpv");
        assert_eq!(player_name("vlc"), "vlc");
        assert_eq!(player_name("io.github.celluloid_player.Celluloid"), "celluloid");
        assert_eq!(player_name("firefox.instance_1_42"), "firefox");
    }
}
