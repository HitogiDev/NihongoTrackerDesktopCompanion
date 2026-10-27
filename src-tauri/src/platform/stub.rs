use super::{Capabilities, WindowInfo};

pub fn capabilities() -> Capabilities {
    Capabilities { os: std::env::consts::OS.into(), ..Default::default() }
}

pub fn game_window_center(_pid: u32) -> Option<(f64, f64)> { None }

pub fn visible_windows() -> Vec<WindowInfo> {
    Vec::new()
}

pub fn foreground_window() -> Option<WindowInfo> {
    None
}

pub fn idle_seconds() -> u64 {
    0
}
