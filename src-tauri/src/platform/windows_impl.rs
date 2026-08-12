use std::cell::RefCell;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible,
};

use super::WindowInfo;

thread_local! {
    static COLLECTED: RefCell<Vec<WindowInfo>> = const { RefCell::new(Vec::new()) };
}

fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let written = GetWindowTextW(hwnd, &mut buf);
        if written <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..written as usize])
    }
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

unsafe extern "system" fn enum_proc(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let title = window_title(hwnd);
    if title.trim().is_empty() {
        return BOOL(1);
    }
    let pid = window_pid(hwnd);
    if pid == 0 {
        return BOOL(1);
    }
    COLLECTED.with(|c| {
        c.borrow_mut().push(WindowInfo {
            pid,
            title,
            process: String::new(),
        })
    });
    BOOL(1)
}

/// Every visible, titled top-level window.
pub fn visible_windows() -> Vec<WindowInfo> {
    COLLECTED.with(|c| c.borrow_mut().clear());
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(0));
    }
    COLLECTED.with(|c| c.borrow_mut().drain(..).collect())
}

/// The window that currently has focus, if any.
pub fn foreground_window() -> Option<WindowInfo> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    let pid = window_pid(hwnd);
    if pid == 0 {
        return None;
    }
    Some(WindowInfo {
        pid,
        title: window_title(hwnd),
        process: String::new(),
    })
}

/// Seconds since the last keyboard / mouse input anywhere in the session.
pub fn idle_seconds() -> u64 {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if !ok.as_bool() {
        return 0;
    }
    let tick = unsafe { windows::Win32::System::SystemInformation::GetTickCount() };
    tick.saturating_sub(info.dwTime) as u64 / 1000
}
