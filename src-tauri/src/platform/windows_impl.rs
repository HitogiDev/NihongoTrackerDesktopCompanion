use std::cell::RefCell;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible,
};

use super::{Capabilities, WindowInfo};

pub fn capabilities() -> Capabilities {
    Capabilities {
        os: "windows".into(),
        session: None,
        window_titles: true,
        foreground: true,
        idle: true,
        media_sessions: false,
        native_hook: true,
        screenshots: true,
        actionable_notifications: true,
        needs_screen_recording: false,
        ctrl_shield: true,
    }
}

// ── Ctrl shield ────────────────────────────────────────────────────────
//
// The reading overlay never takes focus, so its keys go to the game, and a
// novel skips text while Ctrl is held. A low-level keyboard hook sees every key
// before any window does; it lives on its own thread (it needs a message loop)
// only while the overlay is up.

mod ctrl_shield {
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
    use std::sync::Mutex;

    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
        VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED,
        LLKHF_UP, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_QUIT,
    };

    use crate::platform::{ctrl_event, CTRL_PASSED, CTRL_SWALLOWED, CTRL_UP};

    static BLOCKING: AtomicBool = AtomicBool::new(false);
    /// Left and right Ctrl, each a `CTRL_*` state.
    static KEYS: [AtomicU8; 2] = [AtomicU8::new(CTRL_UP), AtomicU8::new(CTRL_UP)];
    /// The hook thread, by id, so it can be told to quit.
    static THREAD: Mutex<Option<u32>> = Mutex::new(None);

    pub fn shield(armed: bool, blocking: bool) {
        BLOCKING.store(armed && blocking, Ordering::Release);
        let mut thread = THREAD.lock().unwrap();
        match (armed, *thread) {
            (true, None) => *thread = start(),
            (false, Some(id)) => {
                unsafe { let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)); }
                *thread = None;
            }
            _ => {}
        }
    }

    fn start() -> Option<u32> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new().name("ctrl-shield".into()).spawn(move || unsafe {
            let mut msg = MSG::default();
            // Creates the thread's queue, so a quit posted right away is not lost.
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let module = GetModuleHandleW(None).map(HINSTANCE::from).unwrap_or_default();
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module, 0);
            let Ok(hook) = hook else {
                log::warn!("Could not install the Ctrl shield: {hook:?}");
                let _ = tx.send(None);
                return;
            };
            let _ = tx.send(Some(GetCurrentThreadId()));
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
            let _ = UnhookWindowsHookEx(hook);
            for key in &KEYS { key.store(CTRL_UP, Ordering::Release); }
        }).ok()?;
        rx.recv().ok().flatten()
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let key = info.vkCode as u16;
            let slot = if key == VK_RCONTROL.0 { Some(1) } else if key == VK_LCONTROL.0 || key == VK_CONTROL.0 { Some(0) } else { None };
            // Synthetic keys come from tools the user runs on purpose.
            let injected = (info.flags & LLKHF_INJECTED).0 != 0;
            let up = (info.flags & LLKHF_UP).0 != 0;
            if let Some(slot) = slot.filter(|_| !injected) {
                let (state, swallow) = ctrl_event(KEYS[slot].load(Ordering::Acquire), up, BLOCKING.load(Ordering::Acquire));
                KEYS[slot].store(state, Ordering::Release);
                if swallow { return LRESULT(1); }
            } else if !injected && !up && !is_modifier(key) && replay_combo(info) {
                return LRESULT(1);
            }
        }
        CallNextHookEx(HHOOK::default(), code, wparam, lparam)
    }

    fn is_modifier(key: u16) -> bool {
        [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_MENU, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN].iter().any(|vk| vk.0 == key)
    }

    /// A key pressed while a swallowed Ctrl is held is a shortcut (the
    /// overlay's own Ctrl+Shift+I among them), and shortcuts must keep working.
    /// So the Ctrl is handed over after all: it and this key are replayed in
    /// order, and from then on that Ctrl's release passes too.
    unsafe fn replay_combo(info: &KBDLLHOOKSTRUCT) -> bool {
        let held: Vec<(usize, VIRTUAL_KEY)> = [(0, VK_LCONTROL), (1, VK_RCONTROL)].into_iter()
            .filter(|(slot, _)| KEYS[*slot].load(Ordering::Acquire) == CTRL_SWALLOWED)
            .collect();
        if held.is_empty() { return false; }
        let input = |vk: VIRTUAL_KEY, scan: u16, extended: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT {
                wVk: vk, wScan: scan,
                dwFlags: if extended { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) },
                time: 0, dwExtraInfo: 0,
            } },
        };
        let mut inputs: Vec<INPUT> = held.iter().map(|(slot, vk)| {
            KEYS[*slot].store(CTRL_PASSED, Ordering::Release);
            input(*vk, 0, *slot == 1)
        }).collect();
        inputs.push(input(VIRTUAL_KEY(info.vkCode as u16), info.scanCode as u16, (info.flags & LLKHF_EXTENDED).0 != 0));
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize == inputs.len()
    }
}

pub fn shield_ctrl(armed: bool, blocking: bool) {
    ctrl_shield::shield(armed, blocking);
}

pub fn game_window_center(pid: u32) -> Option<(f64, f64)> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic};
    let hwnd = unsafe { GetForegroundWindow() };
    if window_pid(hwnd) != pid || unsafe { IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() } { return None; }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect).ok()?; }
    Some(((rect.left as f64 + rect.right as f64) / 2.0, (rect.top as f64 + rect.bottom as f64) / 2.0))
}

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
