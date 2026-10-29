//! Real playback position, when the player is willing to tell us.
//!
//! Wall-clock time is a poor proxy: a paused player, a rewatch or a long
//! pause-for-dinner all inflate it. Two of the players our users actually run
//! expose the truth over a local channel, so ask them:
//!
//! * **mpv** — JSON IPC over a named pipe (Windows) or unix socket. Needs
//!   `input-ipc-server` in `mpv.conf`, which `mpv_config.rs` can add.
//! * **MPC-HC / MPC-BE** — the built-in web interface's `variables.html`.
//!   Needs Options → Player → Web Interface → *Listen on port*.
//!
//! Anything else falls back to elapsed time.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::config::Settings;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum PlaybackSource {
    /// Reported by the player itself.
    Player,
    /// Measured by us, because the player has no API.
    Elapsed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playback {
    pub position_secs: f64,
    pub duration_secs: f64,
    pub paused: bool,
    /// File the player says it is playing, when it says.
    pub file: Option<String>,
}

/// Ask whichever player is on screen where it is. `player` is the display name
/// from `players.rs`.
pub async fn probe(player: &str, settings: &Settings) -> Option<Playback> {
    if !settings.use_player_position {
        return None;
    }
    let result = match player {
        "mpv" | "mpv.net" => mpv(&settings.mpv_ipc_path).await,
        "MPC-HC" | "MPC-BE" => mpc(settings.mpc_web_port).await,
        _ => None,
    };
    // A player that reports a zero-length file is not actually playing.
    result.filter(|p| p.duration_secs > 0.0)
}

// ── mpv ────────────────────────────────────────────────────────────────

const MPV_REQUESTS: [(&str, u64); 4] = [
    ("time-pos", 1),
    ("duration", 2),
    ("pause", 3),
    ("path", 4),
];

async fn mpv(path: &str) -> Option<Playback> {
    let path = normalize_ipc_path(path);
    tokio::time::timeout(Duration::from_millis(600), mpv_inner(&path))
        .await
        .ok()
        .flatten()
}

/// mpv's own sample config suggests `input-ipc-server=/tmp/mpv-socket` "on both
/// Windows and Linux", and on Windows mpv turns that into the named pipe
/// `\\.\pipe\tmp\mpv-socket`. Do the same mapping so whatever the user already
/// has in `mpv.conf` works verbatim.
pub fn normalize_ipc_path(path: &str) -> String {
    if !cfg!(windows) {
        return path.to_string();
    }
    let trimmed = path.trim();
    if trimmed.starts_with(r"\\.\pipe\") {
        return trimmed.to_string();
    }
    let name = trimmed
        .replace('/', r"\")
        .trim_start_matches('\\')
        .to_string();
    format!(r"\\.\pipe\{name}")
}

#[cfg(windows)]
async fn mpv_connect(path: &str) -> Option<tokio::net::windows::named_pipe::NamedPipeClient> {
    tokio::net::windows::named_pipe::ClientOptions::new()
        .open(path)
        .ok()
}

#[cfg(unix)]
async fn mpv_connect(path: &str) -> Option<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(path).await.ok()
}

#[cfg(not(any(windows, unix)))]
async fn mpv_connect(_path: &str) -> Option<tokio::io::Empty> {
    None
}

async fn mpv_inner(path: &str) -> Option<Playback> {
    let stream = mpv_connect(path).await?;
    let (read_half, mut write_half) = tokio::io::split(stream);

    for (property, id) in MPV_REQUESTS {
        let line = format!(
            "{{\"command\":[\"get_property\",\"{property}\"],\"request_id\":{id}}}\n"
        );
        write_half.write_all(line.as_bytes()).await.ok()?;
    }
    write_half.flush().await.ok()?;

    let mut reader = BufReader::new(read_half);
    let mut position = None;
    let mut duration = None;
    let mut paused = false;
    let mut file = None;

    // mpv also pushes unsolicited events on this socket, so read until every
    // request_id has come back rather than assuming four lines.
    for _ in 0..40 {
        let mut line = String::new();
        if reader.read_line(&mut line).await.ok()? == 0 {
            break;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = value.get("request_id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let data = value.get("data");
        match id {
            1 => position = data.and_then(|v| v.as_f64()),
            2 => duration = data.and_then(|v| v.as_f64()),
            3 => paused = data.and_then(|v| v.as_bool()).unwrap_or(false),
            4 => file = data.and_then(|v| v.as_str()).map(|s| s.to_string()),
            _ => continue,
        }
        if position.is_some() && duration.is_some() && file.is_some() {
            break;
        }
    }

    Some(Playback {
        position_secs: position?,
        duration_secs: duration?,
        paused,
        file,
    })
}

// ── MPC-HC / MPC-BE ────────────────────────────────────────────────────

async fn mpc(port: u16) -> Option<Playback> {
    let url = format!("http://127.0.0.1:{port}/variables.html");
    let body = crate::api::local_http()
        .get(url)
        .timeout(Duration::from_millis(600))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;

    // Values are in milliseconds; state is 0 stopped / 1 paused / 2 playing.
    let position = mpc_field(&body, "position")?.parse::<f64>().ok()? / 1000.0;
    let duration = mpc_field(&body, "duration")?.parse::<f64>().ok()? / 1000.0;
    let state = mpc_field(&body, "state").and_then(|s| s.parse::<i32>().ok());

    Some(Playback {
        position_secs: position,
        duration_secs: duration,
        paused: state != Some(2),
        file: mpc_field(&body, "file"),
    })
}

fn mpc_field(body: &str, id: &str) -> Option<String> {
    let needle = format!("id=\"{id}\">");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let end = rest.find("</p>")?;
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<p id="filepatharg">C:\a.mkv</p><p id="file">a.mkv</p>
<p id="state">2</p><p id="statestring">Playing</p>
<p id="position">125000</p><p id="positionstring">02:05</p>
<p id="duration">1420000</p><p id="durationstring">23:40</p>"#;

    #[test]
    fn parses_mpc_variables_page() {
        assert_eq!(mpc_field(SAMPLE, "position").as_deref(), Some("125000"));
        assert_eq!(mpc_field(SAMPLE, "duration").as_deref(), Some("1420000"));
        assert_eq!(mpc_field(SAMPLE, "state").as_deref(), Some("2"));
        assert_eq!(mpc_field(SAMPLE, "file").as_deref(), Some("a.mkv"));
        assert_eq!(mpc_field(SAMPLE, "nope"), None);
    }

    #[cfg(windows)]
    #[test]
    fn maps_unix_style_ipc_path_to_a_named_pipe() {
        // Verified against mpv 0.x on Windows: the pipe really is
        // \\.\pipe\tmp\mpv-socket for input-ipc-server=/tmp/mpv-socket.
        assert_eq!(
            normalize_ipc_path("/tmp/mpv-socket"),
            r"\\.\pipe\tmp\mpv-socket"
        );
        assert_eq!(normalize_ipc_path("mpvsocket"), r"\\.\pipe\mpvsocket");
        assert_eq!(
            normalize_ipc_path(r"\\.\pipe\mpvsocket"),
            r"\\.\pipe\mpvsocket"
        );
    }

    #[test]
    fn mpc_state_other_than_playing_counts_as_paused() {
        let paused = SAMPLE.replace(r#"id="state">2"#, r#"id="state">1"#);
        assert_eq!(mpc_field(&paused, "state").as_deref(), Some("1"));
    }
}
