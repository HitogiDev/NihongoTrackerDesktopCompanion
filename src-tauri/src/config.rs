use serde::{Deserialize, Serialize};

fn d_api_url() -> String {
    "https://nihongotracker.app".into()
}
fn d_true() -> bool {
    true
}
fn d_threshold() -> u8 {
    70
}
fn d_episode_minutes() -> u32 {
    24
}
fn d_afk() -> u64 {
    120
}
fn d_ws() -> String {
    "ws://localhost:6677".into()
}
fn d_poll() -> u64 {
    1000
}
fn d_font_size() -> u32 {
    24
}
fn d_font_family() -> String {
    "Noto Sans JP".into()
}
fn d_line_spacing() -> f32 {
    1.5
}
fn d_theme() -> String {
    "dark".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Base origin of the NihongoTracker instance, without `/api`.
    #[serde(default = "d_api_url")]
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub username: Option<String>,

    // ── Anime detection ────────────────────────────────────────────────
    #[serde(default = "d_true")]
    pub detect_players: bool,
    /// `false` → ask before sending the log (default, Taiga-like).
    pub anime_auto_log: bool,
    /// Share of the episode that must elapse before it counts as watched.
    #[serde(default = "d_threshold")]
    pub anime_threshold_percent: u8,
    /// Fallback episode length when the media entry has none.
    #[serde(default = "d_episode_minutes")]
    pub anime_episode_minutes: u32,
    /// Send the measured watch time alongside the episode count. Off by default:
    /// the site already credits 24 min per episode.
    pub anime_log_time: bool,

    // ── Visual novels ──────────────────────────────────────────────────
    #[serde(default = "d_true")]
    pub detect_visual_novels: bool,
    /// Stop counting after this many seconds without input. 0 disables.
    #[serde(default = "d_afk")]
    pub afk_timeout_secs: u64,
    /// Only count time while the VN window is focused.
    #[serde(default = "d_true")]
    pub vn_require_focus: bool,
    /// Open the texthooker automatically when a registered VN starts.
    #[serde(default = "d_true")]
    pub vn_open_texthooker: bool,
    /// Ask before logging a VN session instead of logging on exit.
    #[serde(default = "d_true")]
    pub vn_confirm_before_log: bool,

    // ── Texthooker ─────────────────────────────────────────────────────
    #[serde(default = "d_ws")]
    pub texthooker_ws_url: String,
    #[serde(default = "d_true")]
    pub texthooker_autoconnect: bool,
    pub texthooker_prevent_duplicates: bool,
    #[serde(default = "d_font_size")]
    pub texthooker_font_size: u32,
    #[serde(default = "d_font_family")]
    pub texthooker_font_family: String,
    #[serde(default = "d_line_spacing")]
    pub texthooker_line_spacing: f32,
    pub texthooker_vertical: bool,

    // ── App ────────────────────────────────────────────────────────────
    #[serde(default = "d_poll")]
    pub poll_interval_ms: u64,
    pub autostart: bool,
    pub start_minimized: bool,
    #[serde(default = "d_true")]
    pub minimize_to_tray: bool,
    #[serde(default = "d_true")]
    pub notifications: bool,
    #[serde(default = "d_theme")]
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_base_url: d_api_url(),
            api_key: None,
            username: None,
            detect_players: true,
            anime_auto_log: false,
            anime_threshold_percent: d_threshold(),
            anime_episode_minutes: d_episode_minutes(),
            anime_log_time: false,
            detect_visual_novels: true,
            afk_timeout_secs: d_afk(),
            vn_require_focus: true,
            vn_open_texthooker: true,
            vn_confirm_before_log: true,
            texthooker_ws_url: d_ws(),
            texthooker_autoconnect: true,
            texthooker_prevent_duplicates: false,
            texthooker_font_size: d_font_size(),
            texthooker_font_family: d_font_family(),
            texthooker_line_spacing: d_line_spacing(),
            texthooker_vertical: false,
            poll_interval_ms: d_poll(),
            autostart: false,
            start_minimized: false,
            minimize_to_tray: true,
            notifications: true,
            theme: d_theme(),
        }
    }
}

impl Settings {
    pub fn api_root(&self) -> String {
        format!("{}/api", self.api_base_url.trim_end_matches('/'))
    }

    pub fn is_authenticated(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn api_root_tolerates_trailing_slash() {
        let mut s = Settings::default();
        s.api_base_url = "https://nihongotracker.app/".into();
        assert_eq!(s.api_root(), "https://nihongotracker.app/api");
    }

    #[test]
    fn blank_key_is_not_authenticated() {
        let mut s = Settings::default();
        assert!(!s.is_authenticated());
        s.api_key = Some("   ".into());
        assert!(!s.is_authenticated());
        s.api_key = Some("ntk_abc".into());
        assert!(s.is_authenticated());
    }

    #[test]
    fn unknown_fields_fall_back_to_defaults() {
        // Settings written by an older build must still load.
        let parsed: Settings = serde_json::from_str(r#"{"api_key":"ntk_x"}"#).unwrap();
        assert_eq!(parsed.anime_threshold_percent, 70);
        assert_eq!(parsed.texthooker_ws_url, "ws://localhost:6677");
    }
}
