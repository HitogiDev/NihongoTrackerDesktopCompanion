//! Known video players and how to recover the played file name from their
//! window title. Same idea as Taiga's `players.anime` data file, but inlined.

/// `(executable, display name, affixes to strip from the window title)`
const PLAYERS: &[(&str, &str, &[&str])] = &[
    ("mpv.exe", "mpv", &[" - mpv"]),
    ("mpvnet.exe", "mpv.net", &[" - mpv.net", " - mpv"]),
    ("mpc-hc.exe", "MPC-HC", &[" - MPC-HC", " - MPC-HC x64"]),
    ("mpc-hc64.exe", "MPC-HC", &[" - MPC-HC", " - MPC-HC x64"]),
    ("mpc-be.exe", "MPC-BE", &[" - MPC-BE", " - MPC-BE x64"]),
    ("mpc-be64.exe", "MPC-BE", &[" - MPC-BE", " - MPC-BE x64"]),
    ("vlc.exe", "VLC", &[" - VLC media player"]),
    ("potplayer.exe", "PotPlayer", &[" - PotPlayer"]),
    ("potplayermini.exe", "PotPlayer", &[" - PotPlayer"]),
    ("potplayermini64.exe", "PotPlayer", &[" - PotPlayer"]),
    ("gom.exe", "GOM Player", &[" - GOM Player"]),
    ("kmplayer.exe", "KMPlayer", &[" - KMPlayer"]),
    ("smplayer.exe", "SMPlayer", &[" - SMPlayer"]),
    ("mpv-wrapper.exe", "mpv", &[" - mpv"]),
    ("celluloid", "Celluloid", &[" - Celluloid"]),
    ("io.github.celluloid_player.celluloid", "Celluloid", &[]),
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
    "iina",
];

pub struct Player {
    pub name: &'static str,
    affixes: &'static [&'static str],
}

pub fn lookup(process: &str) -> Option<Player> {
    let process = process.to_ascii_lowercase();
    PLAYERS
        .iter()
        .find(|(exe, _, _)| *exe == process)
        .map(|(_, name, affixes)| Player {
            name,
            affixes: *affixes,
        })
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
    fn handles_mpc_prefix_form() {
        let p = lookup("mpc-hc64.exe").unwrap();
        assert_eq!(
            p.extract_file_name("MPC-HC - Show 05.mkv").as_deref(),
            Some("Show 05.mkv")
        );
    }
}
