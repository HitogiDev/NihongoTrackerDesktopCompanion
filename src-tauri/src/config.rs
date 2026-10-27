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
/// What mpv's own sample `mpv.conf` suggests, and what most people already have.
/// On Windows `playback::normalize_ipc_path` maps it to the real named pipe.
fn d_mpv_ipc() -> String {
    "/tmp/mpv-socket".into()
}
fn d_mpc_port() -> u16 {
    13579
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
    "Noto Sans JP Local".into()
}
fn d_line_spacing() -> f32 {
    1.5
}
fn d_line_gap() -> u32 {
    16
}
fn d_dictionary_popup_delay_ms() -> u32 {
    75
}
fn d_intelligence_mode() -> String {
    "automatic".into()
}
fn d_intelligence_manual_threshold() -> u32 {
    40
}
fn d_intelligence_afk_threshold() -> u32 {
    300
}
fn d_hooker_theme() -> String {
    "system".into()
}
fn d_room_mode() -> String {
    "local".into()
}
fn d_theme() -> String {
    "dark".into()
}
fn d_streak_hours() -> u8 {
    3
}
/// Asset key uploaded to the Discord application, used whenever there is no
/// cover to show (or when showing one would leak something).
fn d_discord_image() -> String {
    "nihongotracker".into()
}
fn d_discord_app_id() -> String { "1378024719672016977".into() }
fn d_anki_deck() -> String {
    "Default".into()
}
fn d_anki_model() -> String { "Basic".into() }
fn d_anki_field_sources() -> std::collections::BTreeMap<String, String> {
    [("Front".into(), "word".into()), ("Back".into(), "meaning".into())].into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Base origin of the NihongoTracker instance, without `/api`.
    #[serde(default = "d_api_url")]
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub username: Option<String>,
    #[serde(default = "d_true")]
    pub auto_reconnect: bool,

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
    /// Ask mpv / MPC where the episode actually is, instead of timing the window.
    #[serde(default = "d_true")]
    pub use_player_position: bool,
    /// mpv's `input-ipc-server` path.
    #[serde(default = "d_mpv_ipc")]
    pub mpv_ipc_path: String,
    /// Port of the MPC-HC / MPC-BE web interface.
    #[serde(default = "d_mpc_port")]
    pub mpc_web_port: u16,

    // ── Visual novels ──────────────────────────────────────────────────
    #[serde(default = "d_true")]
    pub detect_visual_novels: bool,
    /// Track registered games without opening the texthooker automatically.
    #[serde(default = "d_true")]
    pub detect_games: bool,
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
    /// Show the floating session window above other applications.
    #[serde(default)]
    pub overlay_enabled: bool,
    #[serde(default = "d_anki_deck")]
    pub anki_deck: String,
    #[serde(default = "d_anki_model")]
    pub anki_model: String,
    #[serde(default = "d_anki_field_sources")]
    pub anki_field_sources: std::collections::BTreeMap<String, String>,

    // ── Texthooker ─────────────────────────────────────────────────────
    #[serde(default = "d_ws")]
    pub texthooker_ws_url: String,
    #[serde(default = "d_true")]
    pub texthooker_autoconnect: bool,
    /// Take lines from the clipboard as well. Replaces the browser-extension
    /// paste path the web texthooker relies on.
    pub texthooker_clipboard: bool,
    pub texthooker_prevent_duplicates: bool,
    /// Drop Chinese lines from hooks that send a translation along with the
    /// Japanese (AIR and other dual-language releases). Every input source.
    pub texthooker_filter_chinese: bool,
    pub native_hook_text_filters: crate::text_filters::NativeHookTextFilters,
    #[serde(default = "d_font_size")]
    pub texthooker_font_size: u32,
    #[serde(default = "d_font_family")]
    pub texthooker_font_family: String,
    #[serde(default = "d_line_spacing")]
    pub texthooker_line_spacing: f32,
    #[serde(default = "d_line_gap")]
    pub texthooker_line_gap: u32,
    pub texthooker_vertical: bool,
    /// Theme applied only inside the reader, like the web client's own picker.
    #[serde(default = "d_hooker_theme")]
    pub texthooker_theme: String,
    #[serde(default = "d_true")]
    pub texthooker_show_timer: bool,
    #[serde(default = "d_true")]
    pub texthooker_show_speed: bool,
    #[serde(default = "d_true")]
    pub texthooker_show_characters: bool,
    #[serde(default = "d_true")]
    pub texthooker_show_lines: bool,
    pub texthooker_force_topbar_vertical: bool,
    pub texthooker_topbar_hidden: bool,
    #[serde(default)]
    pub texthooker_custom_css: String,
    /// Seconds without a new line before the timer pauses itself. 0 disables.
    #[serde(default = "d_afk")]
    pub texthooker_auto_pause_timeout: u64,
    pub texthooker_autostart_by_line: bool,
    pub texthooker_autostart_by_clipboard: bool,
    #[serde(default = "d_true")]
    pub texthooker_allow_line_during_pause: bool,
    #[serde(default = "d_true")]
    pub texthooker_allow_clipboard_during_pause: bool,
    pub texthooker_pause_on_disconnect: bool,
    /// Delay before showing a dictionary lookup while holding its hotkey.
    #[serde(default = "d_dictionary_popup_delay_ms")]
    pub dictionary_popup_delay_ms: u32,
    pub reading_overlay: ReadingOverlaySettings,
    #[serde(default = "d_intelligence_mode")]
    pub texthooker_intelligence_mode: String,
    #[serde(default = "d_intelligence_manual_threshold")]
    pub texthooker_intelligence_manual_threshold_seconds: u32,
    #[serde(default = "d_intelligence_afk_threshold")]
    pub texthooker_intelligence_afk_threshold_seconds: u32,
    /// Shared-room mode: `local`, `host` or `guest`.
    #[serde(default = "d_room_mode")]
    pub texthooker_room_mode: String,
    #[serde(default)]
    pub texthooker_room_id: String,

    // ── App ────────────────────────────────────────────────────────────
    #[serde(default = "d_poll")]
    pub poll_interval_ms: u64,
    pub autostart: bool,
    pub start_minimized: bool,
    #[serde(default = "d_true")]
    pub minimize_to_tray: bool,
    #[serde(default = "d_true")]
    pub notifications: bool,
    /// Look for a new GitHub release on start and every few hours. Installing
    /// always waits for the user.
    #[serde(default = "d_true")]
    pub auto_update_check: bool,
    /// Warn when the streak is about to be lost. On by default — a reminder
    /// nobody asked for is annoying, but a streak lost in silence is worse.
    #[serde(default = "d_true")]
    pub streak_reminder: bool,
    /// How many hours before the user's midnight the warning may fire.
    #[serde(default = "d_streak_hours")]
    pub streak_reminder_hours: u8,
    /// Extra fixed local time as `HH:MM`; empty disables it.
    #[serde(default)]
    pub streak_reminder_at: String,

    // ── Discord Rich Presence ──────────────────────────────────────────
    /// Opt-in, and off by default: showing what someone reads is their call.
    #[serde(default)]
    pub discord_rpc: bool,
    /// Application id from the Discord Developer Portal. Without it there is
    /// nothing to connect to.
    #[serde(default = "d_discord_app_id")]
    pub discord_app_id: String,
    /// Publish the activity but never a title or a cover, whatever it is.
    #[serde(default)]
    pub discord_generic_mode: bool,
    /// Add the "view profile" button. It points at a public profile, so it can
    /// be turned off on its own.
    #[serde(default = "d_true")]
    pub discord_profile_button: bool,
    #[serde(default = "d_discord_image")]
    pub discord_large_image: String,
    /// Keep an explicit "idling" presence while a VN is detected but out of
    /// focus (or AFK) instead of wiping it. Off restores the privacy-first
    /// behaviour of clearing the presence whenever time is not being counted.
    #[serde(default = "d_true")]
    pub discord_show_idle: bool,
    #[serde(default)]
    pub share_activity: bool,
    #[serde(default = "d_theme")]
    pub theme: String,
    /// UI language: `en` or `es`, same pair the web app supports. Empty means
    /// "follow the system", resolved by the frontend on first run.
    #[serde(default)]
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_base_url: d_api_url(),
            api_key: None,
            username: None,
            auto_reconnect: true,
            detect_players: true,
            anime_auto_log: false,
            anime_threshold_percent: d_threshold(),
            anime_episode_minutes: d_episode_minutes(),
            anime_log_time: false,
            use_player_position: true,
            mpv_ipc_path: d_mpv_ipc(),
            mpc_web_port: d_mpc_port(),
            detect_visual_novels: true,
            detect_games: true,
            afk_timeout_secs: d_afk(),
            vn_require_focus: true,
            vn_open_texthooker: true,
            vn_confirm_before_log: true,
            overlay_enabled: false,
            anki_deck: d_anki_deck(),
            anki_model: d_anki_model(),
            anki_field_sources: d_anki_field_sources(),
            texthooker_ws_url: d_ws(),
            texthooker_autoconnect: true,
            texthooker_clipboard: false,
            texthooker_prevent_duplicates: false,
            texthooker_filter_chinese: false,
            native_hook_text_filters: Default::default(),
            texthooker_font_size: d_font_size(),
            texthooker_font_family: d_font_family(),
            texthooker_line_spacing: d_line_spacing(),
            texthooker_line_gap: d_line_gap(),
            texthooker_vertical: false,
            texthooker_theme: d_hooker_theme(),
            texthooker_show_timer: true,
            texthooker_show_speed: true,
            texthooker_show_characters: true,
            texthooker_show_lines: true,
            texthooker_force_topbar_vertical: false,
            texthooker_topbar_hidden: false,
            texthooker_custom_css: String::new(),
            texthooker_auto_pause_timeout: d_afk(),
            texthooker_autostart_by_line: false,
            texthooker_autostart_by_clipboard: false,
            texthooker_allow_line_during_pause: true,
            texthooker_allow_clipboard_during_pause: true,
            texthooker_pause_on_disconnect: false,
            dictionary_popup_delay_ms: d_dictionary_popup_delay_ms(),
            reading_overlay: ReadingOverlaySettings::default(),
            texthooker_intelligence_mode: d_intelligence_mode(),
            texthooker_intelligence_manual_threshold_seconds: d_intelligence_manual_threshold(),
            texthooker_intelligence_afk_threshold_seconds: d_intelligence_afk_threshold(),
            texthooker_room_mode: d_room_mode(),
            texthooker_room_id: String::new(),
            poll_interval_ms: d_poll(),
            autostart: false,
            start_minimized: false,
            minimize_to_tray: true,
            notifications: true,
            auto_update_check: true,
            streak_reminder: true,
            streak_reminder_hours: d_streak_hours(),
            streak_reminder_at: String::new(),
            discord_rpc: false,
            discord_app_id: d_discord_app_id(),
            discord_generic_mode: false,
            discord_profile_button: true,
            discord_large_image: d_discord_image(),
            discord_show_idle: true,
            share_activity: false,
            theme: d_theme(),
            language: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ReadingOverlaySettings {
    pub font_size: u32,
    pub font_family: String,
    pub font_weight: u16,
    pub text_outline_width: f32,
    pub text_outline_color: String,
    pub text_shadow_color: String,
    pub text_shadow_opacity: u32,
    pub text_shadow_blur: u32,
    pub center_text: bool,
    pub opacity: u32,
    pub idle_opacity: u32,
    pub text_visibility: OverlayTextVisibility,
    pub always_on_top: bool,
    pub hide_when_unfocused: bool,
    pub toolbar_title_hover_only: bool,
    pub lookup_delay_ms: u32,
    pub popup_enabled: bool,
    pub popup_background_opacity: u32,
    pub popup_close_delay_ms: u32,
    pub toggle_shortcut: String,
    pub interaction_shortcut: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayTextVisibility {
    #[default]
    Always,
    HoverOnly,
    DimWhenIdle,
}

impl Default for ReadingOverlaySettings {
    fn default() -> Self {
        Self {
            font_size: 24,
            font_family: d_font_family(),
            font_weight: 400,
            text_outline_width: 0.0,
            text_outline_color: "#ffffff".into(),
            text_shadow_color: "#000000".into(),
            text_shadow_opacity: 0,
            text_shadow_blur: 6,
            center_text: false,
            opacity: 65,
            idle_opacity: 35,
            text_visibility: OverlayTextVisibility::Always,
            always_on_top: true,
            hide_when_unfocused: true,
            toolbar_title_hover_only: false,
            lookup_delay_ms: d_dictionary_popup_delay_ms(),
            popup_enabled: true,
            popup_background_opacity: 100,
            popup_close_delay_ms: 500,
            toggle_shortcut: "Ctrl+Shift+O".into(),
            interaction_shortcut: "Ctrl+Shift+I".into(),
        }
    }
}

impl ReadingOverlaySettings {
    pub fn normalize_text_style(&mut self) {
        self.popup_background_opacity = self.popup_background_opacity.min(100);
        self.popup_close_delay_ms = self.popup_close_delay_ms.min(5000);
        self.text_outline_width = if self.text_outline_width.is_finite() {
            self.text_outline_width.clamp(0.0, 6.0)
        } else { 0.0 };
        self.text_shadow_opacity = self.text_shadow_opacity.min(100);
        self.text_shadow_blur = self.text_shadow_blur.min(40);
        for (color, fallback) in [(&mut self.text_outline_color, "#ffffff"), (&mut self.text_shadow_color, "#000000")] {
            if color.len() != 7 || !color.starts_with('#') || !color[1..].bytes().all(|byte| byte.is_ascii_hexdigit()) {
                *color = fallback.into();
            }
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
    fn overlay_text_style_preserves_defaults_and_limits_saved_values() {
        let legacy: super::ReadingOverlaySettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.text_outline_width, 0.0);
        assert_eq!(legacy.text_shadow_opacity, 0);
        assert!(legacy.popup_enabled);
        assert_eq!(legacy.popup_background_opacity, 100);
        assert_eq!(legacy.popup_close_delay_ms, 500);
        let mut style = super::ReadingOverlaySettings {
            popup_background_opacity: 200, popup_close_delay_ms: 6000,
            text_outline_width: 20.0, text_outline_color: "invalid".into(),
            text_shadow_color: "#123abc".into(), text_shadow_opacity: 200, text_shadow_blur: 100,
            ..Default::default()
        };
        style.normalize_text_style();
        assert_eq!(style.text_outline_width, 6.0);
        assert_eq!(style.text_outline_color, "#ffffff");
        assert_eq!(style.text_shadow_color, "#123abc");
        assert_eq!(style.text_shadow_opacity, 100);
        assert_eq!(style.text_shadow_blur, 40);
        assert_eq!(style.popup_background_opacity, 100);
        assert_eq!(style.popup_close_delay_ms, 5000);
    }

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
        assert!(parsed.auto_reconnect);
        assert_eq!(parsed.reading_overlay.font_size, 24);
        assert_eq!(parsed.reading_overlay.opacity, 65);
        assert_eq!(parsed.reading_overlay.font_weight, 400);
        assert_eq!(parsed.reading_overlay.idle_opacity, 35);
        assert!(parsed.reading_overlay.always_on_top);
        assert_eq!(parsed.reading_overlay.toggle_shortcut, "Ctrl+Shift+O");
    }
}
