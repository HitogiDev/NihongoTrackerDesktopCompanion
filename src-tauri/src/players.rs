//! Known video players and how to recover the played file name from their
//! window title. Same idea as Taiga's `players.anime` data file, but inlined.

/// `(executable without `.exe`, display name, affixes to strip from the window
/// title)`. The same key matches the Windows image name (`mpv.exe`), the Linux
/// and macOS executable (`mpv`, `IINA`) and the MPRIS name (`vlc`).
const PLAYERS: &[(&str, &str, &[&str])] = &[
    ("mpv", "mpv", &[" - mpv"]),
    ("mpvnet", "mpv.net", &[" - mpv.net", " - mpv"]),
    ("mpc-hc", "MPC-HC", &[" - MPC-HC", " - MPC-HC x64"]),
    ("mpc-hc64", "MPC-HC", &[" - MPC-HC", " - MPC-HC x64"]),
    ("mpc-be", "MPC-BE", &[" - MPC-BE", " - MPC-BE x64"]),
    ("mpc-be64", "MPC-BE", &[" - MPC-BE", " - MPC-BE x64"]),
    ("vlc", "VLC", &[" - VLC media player"]),
    ("potplayer", "PotPlayer", &[" - PotPlayer"]),
    ("potplayermini", "PotPlayer", &[" - PotPlayer"]),
    ("potplayermini64", "PotPlayer", &[" - PotPlayer"]),
    ("gom", "GOM Player", &[" - GOM Player"]),
    ("kmplayer", "KMPlayer", &[" - KMPlayer"]),
    ("smplayer", "SMPlayer", &[" - SMPlayer"]),
    ("mpv-wrapper", "mpv", &[" - mpv"]),
    ("celluloid", "Celluloid", &[" - Celluloid"]),
    ("io.github.celluloid_player.celluloid", "Celluloid", &[]),
    ("haruna", "Haruna", &[" — Haruna", " - Haruna"]),
    ("iina", "IINA", &[]),
];

/// Titles a player shows when nothing is playing.
const IDLE_TITLES: &[&str] = &[
    "mpv",
    "mpv.net",
    "vlc media player",
    "vlc",
    "mpc-hc",
    "mpc-hc x64",
    "mpc-be",
    "mpc-be x64",
    "potplayer",
    "gom player",
    "kmplayer",
    "smplayer",
    "celluloid",
    "haruna",
    "iina",
];

pub struct Player {
    pub name: &'static str,
    affixes: &'static [&'static str],
}

pub fn lookup(process: &str) -> Option<Player> {
    let process = process.to_ascii_lowercase();
    let process = process.strip_suffix(".exe").unwrap_or(&process);
    PLAYERS
        .iter()
        .find(|(exe, _, _)| *exe == process)
        .map(|(_, name, affixes)| Player {
            name,
            affixes: *affixes,
        })
}

/// The file a media-bus player is playing: the last segment of a `file://`
/// URL, decoded, which is the same release name a window title would show.
/// Falls back to the track title when there is no URL (streams, some players).
pub fn media_file_name(url: Option<&str>, title: Option<&str>) -> Option<String> {
    let from_url = url
        .and_then(|url| url.strip_prefix("file://"))
        .and_then(|path| path.rsplit('/').next())
        .map(percent_decode)
        .filter(|name| !name.trim().is_empty());
    from_url.or_else(|| title.map(str::trim).filter(|title| !title.is_empty()).map(str::to_string))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Player {
    /// Strip the player's own branding off the window title. Returns `None`
    /// when the player is sitting idle with no file open.
    pub fn extract_file_name(&self, title: &str) -> Option<String> {
        let mut cleaned = title.trim().to_string();

        for affix in self.affixes {
            if let Some(rest) = cleaned.strip_suffix(affix) {
                cleaned = rest.trim().to_string();
                break;
            }
            // MPC variants sometimes render as "MPC-HC - <file>".
            let prefix = affix.trim_start_matches(" - ");
            if let Some(rest) = cleaned.strip_prefix(&format!("{prefix} - ")) {
                cleaned = rest.trim().to_string();
                break;
            }
        }

        // VLC/PotPlayer prepend playback state markers on some builds.
        for marker in ["[Paused] ", "[Playing] ", "*"] {
            cleaned = cleaned.trim_start_matches(marker).trim().to_string();
        }

        if cleaned.is_empty() {
            return None;
        }
        if IDLE_TITLES.contains(&cleaned.to_ascii_lowercase().as_str()) {
            return None;
        }
        Some(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_player_branding() {
        let p = lookup("mpv.exe").unwrap();
        assert_eq!(
            p.extract_file_name("[Group] Show - 03 [1080p].mkv - mpv")
                .as_deref(),
            Some("[Group] Show - 03 [1080p].mkv")
        );
    }

    #[test]
    fn detects_idle_player() {
        let p = lookup("vlc.exe").unwrap();
        assert_eq!(p.extract_file_name("VLC media player"), None);
    }

    #[test]
    fn matches_executables_with_or_without_the_windows_extension() {
        assert_eq!(lookup("mpv.exe").map(|p| p.name), Some("mpv"));
        assert_eq!(lookup("mpv").map(|p| p.name), Some("mpv"));
        assert_eq!(lookup("IINA").map(|p| p.name), Some("IINA"));
        assert_eq!(lookup("haruna").map(|p| p.name), Some("Haruna"));
        assert!(lookup("firefox").is_none());
    }

    #[test]
    fn media_bus_urls_give_the_release_file_name() {
        assert_eq!(
            media_file_name(Some("file:///home/u/Anime/%5BGroup%5D%20Show%20-%2003%20%5B1080p%5D.mkv"), Some("Show")).as_deref(),
            Some("[Group] Show - 03 [1080p].mkv")
        );
        assert_eq!(
            media_file_name(Some("file:///m/%E8%91%AC%E9%80%81%E3%81%AE%E3%83%95%E3%83%AA%E3%83%BC%E3%83%AC%E3%83%B3%2005.mkv"), None).as_deref(),
            Some("葬送のフリーレン 05.mkv")
        );
        assert_eq!(media_file_name(Some("https://example.com/watch"), Some(" Show 05 ")).as_deref(), Some("Show 05"));
        assert_eq!(media_file_name(None, None), None);
        assert_eq!(media_file_name(Some("file:///tmp/50%.mkv"), None).as_deref(), Some("50%.mkv"));
    }

    #[test]
    fn handles_mpc_prefix_form() {
        let p = lookup("mpc-hc64.exe").unwrap();
        assert_eq!(
            p.extract_file_name("MPC-HC - Show 05.mkv").as_deref(),
            Some("Show 05.mkv")
        );
    }
}
