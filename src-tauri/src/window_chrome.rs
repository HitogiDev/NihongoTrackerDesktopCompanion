//! Native chrome for the app's windows: a title bar that follows the app's
//! light/dark theme, the right background before the page paints, and Mica
//! behind the main window's sidebar on Windows 11. The page keeps
//! NihongoTracker's palette; this only makes the frame around it native.

use tauri::window::Color;
use tauri::{Manager, Runtime, Theme, WebviewWindowBuilder};

/// daisyUI themes with a dark base. Anything else gets a light title bar.
const DARK_THEMES: &[&str] = &[
    "dark", "synthwave", "halloween", "forest", "dracula", "night", "dim", "sunset", "abyss",
    "black", "luxury", "business", "coffee",
];

/// `base-100` of daisyUI's `dark` and of the app's own `light` theme.
const DARK_BACKGROUND: Color = Color(0x1d, 0x23, 0x2a, 0xff);
const LIGHT_BACKGROUND: Color = Color(0xf7, 0xf9, 0xfb, 0xff);

/// Native theme for a daisyUI theme name. `None` lets the OS decide ("system").
pub fn native_theme(theme: &str) -> Option<Theme> {
    match theme {
        "" | "system" => None,
        theme if DARK_THEMES.contains(&theme) => Some(Theme::Dark),
        _ => Some(Theme::Light),
    }
}

fn background(theme: &str) -> Color {
    match native_theme(theme) {
        Some(Theme::Light) => LIGHT_BACKGROUND,
        _ => DARK_BACKGROUND,
    }
}

/// Title bar theme and start-up background for any app window.
pub fn themed<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    theme: &str,
) -> WebviewWindowBuilder<'a, R, M> {
    builder.theme(native_theme(theme)).background_color(background(theme))
}

/// Mica needs Windows 11 (build 22000). On Windows 10 a transparent window
/// with no material behind it would show the desktop through the sidebar.
#[cfg(windows)]
fn mica_supported() -> bool {
    matches!(tauri_plugin_os::version(), tauri_plugin_os::Version::Semantic(10, 0, build) if build >= 22000)
}

/// The main window: themed, plus Mica where the OS has it. The page learns
/// about it from `data-vibrancy` on `<html>`, set before any script runs, and
/// only then makes the sidebar translucent.
pub fn main_window<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    theme: &str,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(windows)]
    if mica_supported() {
        use tauri::window::{Effect, EffectsBuilder};
        return builder
            .theme(native_theme(theme))
            .transparent(true)
            .effects(EffectsBuilder::new().effect(Effect::Mica).build())
            .initialization_script("document.documentElement.dataset.vibrancy = 'mica';");
    }
    themed(builder, theme)
}

/// Keeps every open window's title bar (and Mica tint) on the app's theme.
pub fn sync_theme<R: Runtime>(app: &tauri::AppHandle<R>, theme: &str) {
    for window in app.webview_windows().values() {
        let _ = window.set_theme(native_theme(theme));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daisyui_themes_map_to_the_native_title_bar() {
        assert_eq!(native_theme("system"), None);
        assert_eq!(native_theme("dark"), Some(Theme::Dark));
        assert_eq!(native_theme("dracula"), Some(Theme::Dark));
        assert_eq!(native_theme("light"), Some(Theme::Light));
        assert_eq!(native_theme("cupcake"), Some(Theme::Light));
    }
}
