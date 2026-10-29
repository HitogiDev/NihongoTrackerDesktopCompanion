//! macOS: CoreGraphics window list and HID idle time.
//!
//! `CGWindowListCopyWindowInfo` lists on-screen windows front to back with
//! their owner's pid and bounds, which is enough for focus (the first normal
//! window belongs to the frontmost app) and for the overlay's placement. Their
//! titles, though, are only filled in once the user grants Screen Recording to
//! the app; until then players can only be followed through their own IPC.
//! Idle time needs no permission.

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::display::CGRect;
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowName,
    kCGWindowOwnerPID,
};

use super::{Capabilities, WindowInfo};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(source_state: i32, event_type: u32) -> f64;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// `kCGEventSourceStateHIDSystemState` and `kCGAnyInputEventType`.
const HID_SYSTEM_STATE: i32 = 1;
const ANY_INPUT_EVENT: u32 = u32::MAX;

pub fn capabilities() -> Capabilities {
    let titles = unsafe { CGPreflightScreenCaptureAccess() };
    Capabilities {
        os: "macos".into(),
        session: None,
        window_titles: titles,
        foreground: true,
        idle: true,
        media_sessions: false,
        native_hook: false,
        screenshots: titles,
        actionable_notifications: false,
        needs_screen_recording: !titles,
        // A CGEventTap that drops events needs the Accessibility permission.
        ctrl_shield: false,
    }
}

pub fn shield_ctrl(_armed: bool, _blocking: bool) {}

/// Shows the system prompt the first time; afterwards macOS only changes its
/// answer from System Settings, and only after the app restarts.
pub fn request_screen_recording() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

struct Entry {
    info: WindowInfo,
    layer: i64,
    bounds: Option<CGRect>,
}

fn number(dict: &CFDictionary<CFString, CFType>, key: &CFString) -> Option<i64> {
    dict.find(key)?.downcast::<CFNumber>()?.to_i64()
}

/// On-screen windows, front to back.
fn windows() -> Vec<Entry> {
    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let Some(list): Option<CFArray> = copy_window_info(options, kCGNullWindowID) else {
        return Vec::new();
    };
    let (pid_key, name_key, layer_key, bounds_key) = unsafe {
        (
            CFString::wrap_under_get_rule(kCGWindowOwnerPID),
            CFString::wrap_under_get_rule(kCGWindowName),
            CFString::wrap_under_get_rule(kCGWindowLayer),
            CFString::wrap_under_get_rule(kCGWindowBounds),
        )
    };
    list.iter()
        .filter_map(|item| {
            let dict: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(*item as _) };
            let pid = u32::try_from(number(&dict, &pid_key)?).ok()?;
            let title = dict
                .find(&name_key)
                .and_then(|value| value.downcast::<CFString>())
                .map(|name| name.to_string())
                .unwrap_or_default();
            let bounds = dict
                .find(&bounds_key)
                .and_then(|value| value.downcast::<CFDictionary>())
                .and_then(|rect| CGRect::from_dict_representation(&rect));
            Some(Entry {
                info: WindowInfo { pid, title, process: String::new() },
                layer: number(&dict, &layer_key).unwrap_or(0),
                bounds,
            })
        })
        .collect()
}

/// Normal application windows live on layer 0; menus, the Dock and the menu
/// bar extras sit above it.
fn normal(entry: &Entry) -> bool {
    entry.layer == 0
}

pub fn visible_windows() -> Vec<WindowInfo> {
    windows()
        .into_iter()
        .filter(normal)
        .filter(|entry| !entry.info.title.trim().is_empty())
        .map(|entry| entry.info)
        .collect()
}

pub fn foreground_window() -> Option<WindowInfo> {
    windows().into_iter().find(normal).map(|entry| entry.info)
}

pub fn game_window_center(pid: u32) -> Option<(f64, f64)> {
    let front = windows().into_iter().find(normal)?;
    if front.info.pid != pid {
        return None;
    }
    let rect = front.bounds?;
    Some((rect.origin.x + rect.size.width / 2.0, rect.origin.y + rect.size.height / 2.0))
}

pub fn idle_seconds() -> u64 {
    let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT) };
    if seconds.is_finite() && seconds > 0.0 { seconds as u64 } else { 0 }
}
