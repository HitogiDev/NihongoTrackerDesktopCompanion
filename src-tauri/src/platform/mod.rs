//! Platform abstraction for window / process inspection.
//!
//! Only Windows is implemented for now; the other targets return empty data so
//! the rest of the app compiles and runs (with detection disabled) everywhere.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WindowInfo {
    pub pid: u32,
    pub title: String,
    /// Executable file name, lowercased (e.g. `mpv.exe`). Filled in by the caller.
    pub process: String,
}

#[cfg(windows)]
mod windows_impl;
#[cfg(windows)]
pub use windows_impl::{foreground_window, idle_seconds, visible_windows};

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::{foreground_window, idle_seconds, visible_windows};

/// True when the platform can actually observe windows.
pub const fn supported() -> bool {
    cfg!(windows)
}
