//! Turns on mpv's JSON IPC for the user, so `playback.rs` can ask where the
//! episode is instead of timing the window.
//!
//! mpv only opens the socket when `mpv.conf` has `input-ipc-server`, and only
//! reads `mpv.conf` at startup. So the setup finds the file mpv will actually
//! read, adds the line (or adopts the one already there) and tells the user to
//! restart the player. It never runs on its own: the UI asks first.
//!
//! What it never does: rewrite what the user wrote. A line already there wins
//! and the companion follows it; a new line goes in once, with a comment, and
//! the original file is kept next to it the first time.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// The line we add, above the option, so whoever opens the file knows why.
const COMMENT: &str = "# Added by NihongoTracker Companion: lets it read the playback position.";
const OPTION: &str = "input-ipc-server";
/// The Flatpak's runtime directory is the only place both sides can reach:
/// `/tmp` inside the sandbox is private.
const FLATPAK_ID: &str = "io.mpv.Mpv";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum MpvSetupAction {
    /// `mpv.conf` has no socket: add the line. mpv needs a restart.
    Add,
    /// `mpv.conf` already opens a socket somewhere else: listen there instead.
    Adopt,
    /// Both sides already agree; if the position is still missing, mpv was
    /// started before the line existed.
    Nothing,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MpvSetup {
    /// The file mpv reads (created if missing).
    pub conf_path: String,
    pub exists: bool,
    /// `input-ipc-server` as the file has it now.
    pub existing_ipc: Option<String>,
    /// Where the companion will listen afterwards (`Settings::mpv_ipc_path`).
    pub ipc_path: String,
    pub action: MpvSetupAction,
    /// Copy of the file before we touched it; only after applying.
    pub backup_path: Option<String>,
}

/// Where mpv looks for its configuration, as far as it matters here.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub windows: bool,
    /// The running player's executable, when it is running.
    pub exe: Option<PathBuf>,
    pub appdata: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_runtime_dir: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Env {
    pub fn current(exe: Option<PathBuf>) -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        Self {
            windows: cfg!(windows),
            exe,
            appdata: var("APPDATA"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            xdg_runtime_dir: var("XDG_RUNTIME_DIR"),
            home: var("HOME").or_else(|| var("USERPROFILE")),
        }
    }

    /// A Flatpak process sees itself under `/app`.
    fn flatpak(&self) -> bool {
        self.exe.as_ref().is_some_and(|exe| exe.starts_with("/app"))
    }
}

/// The `mpv.conf` mpv will read. `player` is the display name from `players.rs`.
///
/// `portable_config` next to the executable overrides everything (scoop and
/// the portable builds use it); otherwise the per-user directory, which for
/// mpv.net is its own and for the Flatpak lives under `~/.var/app`.
pub fn conf_path(env: &Env, player: &str, is_dir: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    if let Some(dir) = env.exe.as_ref().and_then(|exe| exe.parent()) {
        let portable = dir.join("portable_config");
        if is_dir(&portable) {
            return Some(portable.join("mpv.conf"));
        }
    }
    let dir = if env.windows {
        let folder = if player == "mpv.net" { "mpv.net" } else { "mpv" };
        env.appdata.as_ref()?.join(folder)
    } else if env.flatpak() {
        env.home.as_ref()?.join(".var/app").join(FLATPAK_ID).join("config/mpv")
    } else {
        env.xdg_config_home.clone().or_else(|| env.home.as_ref().map(|home| home.join(".config")))?.join("mpv")
    };
    Some(dir.join("mpv.conf"))
}

/// The socket path to write when the file has none: what the companion is
/// already set to, except in a Flatpak, where only the runtime dir is shared.
pub fn socket_for(env: &Env, configured: &str) -> String {
    if env.flatpak() {
        if let Some(runtime) = &env.xdg_runtime_dir {
            return format!("{}/app/{FLATPAK_ID}/mpv-socket", runtime.display());
        }
    }
    let configured = configured.trim();
    if configured.is_empty() { "/tmp/mpv-socket".into() } else { configured.into() }
}

/// Top-level `input-ipc-server` in `text`, the last one if repeated (that is
/// the one mpv keeps). Lines inside a profile other than `[default]` only
/// apply when that profile is loaded, so they do not count.
pub fn existing_ipc(text: &str) -> Option<String> {
    let mut in_profile = false;
    let mut found = None;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_profile = !section.trim().eq_ignore_ascii_case("default");
            continue;
        }
        if in_profile || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        if key.trim().trim_start_matches("--") != OPTION {
            continue;
        }
        // An unquoted value may carry a trailing comment.
        let value = value.trim();
        let value = match value.strip_prefix('"').and_then(|v| v.split_once('"')) {
            Some((quoted, _)) => quoted,
            None => value.split('#').next().unwrap_or("").trim(),
        };
        found = (!value.is_empty()).then(|| value.to_string());
    }
    found
}

/// `text` with our line added at the top level: before the first profile, or
/// at the end when there is none. Keeps the file's BOM and line endings.
pub fn with_ipc_line(text: &str, socket: &str) -> String {
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    };
    let eol = if body.contains("\r\n") { "\r\n" } else { "\n" };
    let value = if socket.contains([' ', '#']) { format!("\"{socket}\"") } else { socket.to_string() };
    let block = format!("{COMMENT}{eol}{OPTION}={value}{eol}");

    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim();
        let section = trimmed.strip_prefix('[').and_then(|l| l.strip_suffix(']'));
        if section.is_some_and(|name| !name.trim().eq_ignore_ascii_case("default")) {
            let (before, after) = body.split_at(offset);
            let gap = if before.is_empty() || before.ends_with(&format!("{eol}{eol}")) { "" } else { eol };
            return format!("{bom}{before}{gap}{block}{eol}{after}");
        }
        offset += line.len();
    }
    let joint = if body.is_empty() || body.ends_with('\n') { "" } else { eol };
    format!("{bom}{body}{joint}{block}")
}

/// What applying would do, without touching anything.
pub fn plan(env: &Env, player: &str, configured_socket: &str) -> Result<MpvSetup, String> {
    let path = conf_path(env, player, |p| p.is_dir()).ok_or("no configuration directory for mpv")?;
    let text = std::fs::read_to_string(&path).ok();
    let existing = text.as_deref().and_then(existing_ipc);
    let (ipc_path, action) = match &existing {
        Some(socket) if same_socket(socket, configured_socket) => (configured_socket.to_string(), MpvSetupAction::Nothing),
        Some(socket) => (socket.clone(), MpvSetupAction::Adopt),
        None => (socket_for(env, configured_socket), MpvSetupAction::Add),
    };
    Ok(MpvSetup {
        conf_path: path.to_string_lossy().into_owned(),
        exists: text.is_some(),
        existing_ipc: existing,
        ipc_path,
        action,
        backup_path: None,
    })
}

/// Carries out [`plan`]. Only `Add` writes; the caller saves `ipc_path` into
/// the settings either way.
pub fn apply(env: &Env, player: &str, configured_socket: &str) -> Result<MpvSetup, String> {
    let mut setup = plan(env, player, configured_socket)?;
    if setup.action != MpvSetupAction::Add {
        return Ok(setup);
    }
    let path = PathBuf::from(&setup.conf_path);
    let current = if setup.exists { std::fs::read_to_string(&path).map_err(|e| e.to_string())? } else { String::new() };
    if setup.exists {
        // Keep the user's own version, not one we already edited.
        let backup = path.with_file_name("mpv.conf.before-nihongotracker");
        if !backup.exists() {
            std::fs::copy(&path, &backup).map_err(|e| e.to_string())?;
        }
        setup.backup_path = Some(backup.to_string_lossy().into_owned());
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, with_ipc_line(&current, &setup.ipc_path)).map_err(|e| e.to_string())?;
    setup.exists = true;
    Ok(setup)
}

/// Whether two spellings reach the same socket (on Windows `/tmp/mpv-socket`
/// and `\\.\pipe\tmp\mpv-socket` are one pipe).
fn same_socket(a: &str, b: &str) -> bool {
    crate::playback::normalize_ipc_path(a) == crate::playback::normalize_ipc_path(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(exe: Option<&str>) -> Env {
        Env {
            windows: true,
            exe: exe.map(PathBuf::from),
            appdata: Some(PathBuf::from(r"C:\Users\u\AppData\Roaming")),
            ..Env::default()
        }
    }

    #[test]
    fn finds_the_file_mpv_reads() {
        let none = |_: &Path| false;
        let env = windows(Some(r"C:\Tools\mpv\mpv.exe"));
        assert_eq!(conf_path(&env, "mpv", none), Some(PathBuf::from(r"C:\Users\u\AppData\Roaming").join("mpv").join("mpv.conf")));
        assert_eq!(conf_path(&env, "mpv.net", none), Some(PathBuf::from(r"C:\Users\u\AppData\Roaming").join("mpv.net").join("mpv.conf")));
        // portable_config next to the executable wins.
        let env = windows(Some("/opt/mpv/mpv.exe"));
        assert_eq!(
            conf_path(&env, "mpv", |p| p.ends_with("portable_config")),
            Some(PathBuf::from("/opt/mpv").join("portable_config").join("mpv.conf"))
        );

        let linux = Env { home: Some("/home/u".into()), ..Env::default() };
        assert_eq!(conf_path(&linux, "mpv", none), Some(PathBuf::from("/home/u/.config/mpv/mpv.conf")));
        let xdg = Env { xdg_config_home: Some("/cfg".into()), ..linux.clone() };
        assert_eq!(conf_path(&xdg, "mpv", none), Some(PathBuf::from("/cfg/mpv/mpv.conf")));
        let flatpak = Env { exe: Some("/app/bin/mpv".into()), ..linux };
        assert_eq!(conf_path(&flatpak, "mpv", none), Some(PathBuf::from("/home/u/.var/app/io.mpv.Mpv/config/mpv/mpv.conf")));
    }

    #[test]
    fn a_flatpak_gets_a_socket_the_host_can_reach() {
        let env = Env { exe: Some("/app/bin/mpv".into()), xdg_runtime_dir: Some("/run/user/1000".into()), ..Env::default() };
        assert_eq!(socket_for(&env, "/tmp/mpv-socket"), "/run/user/1000/app/io.mpv.Mpv/mpv-socket");
        assert_eq!(socket_for(&Env::default(), "/tmp/mine"), "/tmp/mine");
        assert_eq!(socket_for(&Env::default(), "  "), "/tmp/mpv-socket");
    }

    #[test]
    fn reads_the_socket_mpv_would_use() {
        assert_eq!(existing_ipc("input-ipc-server=/tmp/a\n"), Some("/tmp/a".into()));
        assert_eq!(existing_ipc("\u{feff}--input-ipc-server = \"\\\\.\\pipe\\x y\" # pipe\n"), Some(r"\\.\pipe\x y".into()));
        assert_eq!(existing_ipc("input-ipc-server=/tmp/a # old\ninput-ipc-server=/tmp/b\n"), Some("/tmp/b".into()));
        assert_eq!(existing_ipc("#input-ipc-server=/tmp/a\n"), None);
        assert_eq!(existing_ipc("[anime]\ninput-ipc-server=/tmp/a\n"), None);
        assert_eq!(existing_ipc("[anime]\nvolume=50\n[default]\ninput-ipc-server=/tmp/a\n"), Some("/tmp/a".into()));
        assert_eq!(existing_ipc("input-ipc-server=\n"), None);
    }

    #[test]
    fn adds_the_line_at_the_top_level() {
        let added = with_ipc_line("volume=50", "/tmp/mpv-socket");
        assert_eq!(added, format!("volume=50\n{COMMENT}\ninput-ipc-server=/tmp/mpv-socket\n"));
        assert_eq!(existing_ipc(&added), Some("/tmp/mpv-socket".into()));

        let profiles = with_ipc_line("volume=50\r\n\r\n[anime]\r\nsub-auto=all\r\n", "/tmp/s");
        assert!(profiles.starts_with(&format!("volume=50\r\n\r\n{COMMENT}\r\ninput-ipc-server=/tmp/s\r\n\r\n[anime]")));
        assert_eq!(existing_ipc(&profiles), Some("/tmp/s".into()));

        let bom = with_ipc_line("\u{feff}[anime]\n", "/tmp/s");
        assert!(bom.starts_with(&format!("\u{feff}{COMMENT}\n")));
        assert_eq!(with_ipc_line("", "/tmp/a b"), format!("{COMMENT}\ninput-ipc-server=\"/tmp/a b\"\n"));
    }

    #[test]
    fn applying_adds_once_and_keeps_the_original() {
        let dir = std::env::temp_dir().join(format!("ntdc-mpv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = Env { windows: false, xdg_config_home: Some(dir.clone()), ..Env::default() };
        let conf = dir.join("mpv").join("mpv.conf");

        // No file at all: it is created.
        let created = apply(&env, "mpv", "/tmp/mpv-socket").unwrap();
        assert_eq!(created.action, MpvSetupAction::Add);
        assert_eq!(created.backup_path, None);
        assert_eq!(existing_ipc(&std::fs::read_to_string(&conf).unwrap()), Some("/tmp/mpv-socket".into()));
        assert_eq!(apply(&env, "mpv", "/tmp/mpv-socket").unwrap().action, MpvSetupAction::Nothing);

        // Someone else's socket is followed, not replaced.
        std::fs::write(&conf, "input-ipc-server=/tmp/theirs\n").unwrap();
        let adopted = apply(&env, "mpv", "/tmp/mpv-socket").unwrap();
        assert_eq!((adopted.action, adopted.ipc_path.as_str()), (MpvSetupAction::Adopt, "/tmp/theirs"));
        assert_eq!(std::fs::read_to_string(&conf).unwrap(), "input-ipc-server=/tmp/theirs\n");

        std::fs::write(&conf, "volume=50\n").unwrap();
        let added = apply(&env, "mpv", "/tmp/mpv-socket").unwrap();
        let backup = PathBuf::from(added.backup_path.unwrap());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "volume=50\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
