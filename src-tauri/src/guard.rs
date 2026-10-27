#![cfg(windows)]

use parking_lot::Mutex;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, MoveWindow, SetForegroundWindow, SetWindowPos,
    HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
};

const POLL_MS: u64 = 350;
const LAYOUT_POLL_MS: u64 = 200;
/// Left pane share of the primary monitor width.
const LEFT_RATIO: f64 = 0.58;

const WHITELIST: &[&str] = &[
    "interview-meeting",
    "interview_meeting",
    "wemeetapp",
    "wemeet",
    "tencentmeeting",
    "wechatmeeting",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardStatus {
    pub enabled: bool,
    pub pull_back: bool,
    pub violations: u32,
    pub last_process: String,
    pub last_title: String,
    pub foreground_allowed: bool,
    pub monitor_count: u32,
    pub layout_locked: bool,
    pub topmost: bool,
}

#[derive(Clone, Copy)]
struct LockedPane {
    hwnd: isize,
    rect: RECT,
}

#[derive(Default)]
struct GuardInner {
    enabled: bool,
    pull_back: bool,
    violations: u32,
    last_process: String,
    last_title: String,
    foreground_allowed: bool,
    /// When true: keep both panes topmost, full-height, and snap back if dragged.
    layout_locked: bool,
    topmost: bool,
    main_pane: Option<LockedPane>,
    meet_pane: Option<LockedPane>,
}

#[derive(Clone, Default)]
pub struct GuardState {
    inner: Arc<Mutex<GuardInner>>,
}

impl GuardState {
    pub fn status(&self) -> GuardStatus {
        let g = self.inner.lock();
        GuardStatus {
            enabled: g.enabled,
            pull_back: g.pull_back,
            violations: g.violations,
            last_process: g.last_process.clone(),
            last_title: g.last_title.clone(),
            foreground_allowed: g.foreground_allowed,
            monitor_count: count_monitors(),
            layout_locked: g.layout_locked,
            topmost: g.topmost,
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.inner.lock().enabled = enabled;
    }

    pub fn set_pull_back(&self, pull_back: bool) {
        self.inner.lock().pull_back = pull_back;
    }
}

fn exe_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_whitelisted(stem: &str) -> bool {
    WHITELIST.iter().any(|w| stem.contains(w))
}

fn rect_eq(a: &RECT, b: &RECT) -> bool {
    a.left == b.left && a.top == b.top && a.right == b.right && a.bottom == b.bottom
}

unsafe fn process_image_path(hwnd: HWND) -> String {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == 0 {
        return String::new();
    }
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if handle.is_null() {
        return String::new();
    }
    let mut buf = [0u16; 1024];
    let mut size = buf.len() as u32;
    let path = if QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) != 0 {
        String::from_utf16_lossy(&buf[..size as usize])
    } else {
        String::new()
    };
    CloseHandle(handle);
    path
}

unsafe fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    if len <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

fn count_monitors() -> u32 {
    unsafe extern "system" fn callback(
        _hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let count = lparam as *mut u32;
        unsafe {
            *count += 1;
        }
        TRUE
    }

    unsafe {
        let mut count = 0u32;
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(callback),
            &mut count as *mut u32 as isize,
        );
        count.max(1)
    }
}

/// Full primary monitor bounds (covers taskbar area too) for immersive dual-pane cover.
fn primary_monitor_rect() -> RECT {
    unsafe {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            rcWork: RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            dwFlags: 0,
        };
        let hwnd = GetForegroundWindow();
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        GetMonitorInfoW(monitor, &mut info);
        info.rcMonitor
    }
}

unsafe fn force_pane(hwnd: HWND, rect: RECT, topmost: bool) {
    if hwnd.is_null() || IsWindow(hwnd) == 0 {
        return;
    }
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;
    MoveWindow(hwnd, rect.left, rect.top, w, h, 1);
    let z = if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
    SetWindowPos(
        hwnd,
        z,
        rect.left,
        rect.top,
        w,
        h,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}

unsafe fn current_rect(hwnd: HWND) -> Option<RECT> {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if GetWindowRect(hwnd, &mut r) != 0 {
        Some(r)
    } else {
        None
    }
}

pub fn start_guard_loop(app: AppHandle, state: GuardState) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(POLL_MS));

        let (enabled, pull_back) = {
            let g = state.inner.lock();
            (g.enabled, g.pull_back)
        };
        if !enabled {
            continue;
        }

        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                continue;
            }
            let path = process_image_path(hwnd);
            let stem = exe_stem(&path);
            let title = window_title(hwnd);
            let allowed = is_whitelisted(&stem);

            {
                let mut g = state.inner.lock();
                g.last_process = if stem.is_empty() {
                    path.clone()
                } else {
                    format!("{stem}.exe")
                };
                g.last_title = title.clone();
                g.foreground_allowed = allowed;
            }

            if !allowed {
                {
                    let mut g = state.inner.lock();
                    g.violations = g.violations.saturating_add(1);
                }
                let status = state.status();
                let _ = app.emit("guard://violation", &status);
                let _ = app.emit("guard://status", &status);

                if pull_back {
                    if let Some(main) = app.get_window("main") {
                        let _ = main.set_focus();
                        if let Ok(hwnd_main) = main.hwnd() {
                            SetForegroundWindow(hwnd_main.0 as isize as HWND);
                        }
                    }
                }
            } else {
                let status = state.status();
                let _ = app.emit("guard://status", &status);
            }
        }
    });
}

/// Continuously snap dual panes back if the user tries to drag/resize them while locked.
pub fn start_layout_lock_loop(state: GuardState) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(LAYOUT_POLL_MS));
        let (locked, topmost, main, meet) = {
            let g = state.inner.lock();
            (g.layout_locked, g.topmost, g.main_pane, g.meet_pane)
        };
        if !locked {
            continue;
        }
        unsafe {
            if let Some(pane) = main {
                let hwnd = pane.hwnd as HWND;
                if IsWindow(hwnd) != 0 {
                    let drifted = current_rect(hwnd)
                        .map(|r| !rect_eq(&r, &pane.rect))
                        .unwrap_or(true);
                    if drifted || topmost {
                        force_pane(hwnd, pane.rect, topmost);
                    }
                }
            }
            if let Some(pane) = meet {
                let hwnd = pane.hwnd as HWND;
                if IsWindow(hwnd) != 0 {
                    let drifted = current_rect(hwnd)
                        .map(|r| !rect_eq(&r, &pane.rect))
                        .unwrap_or(true);
                    if drifted || topmost {
                        force_pane(hwnd, pane.rect, topmost);
                    }
                }
            }
        }
    });
}

struct EnumData {
    handles: Vec<HWND>,
}

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let data = &mut *(lparam as *mut EnumData);
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
        return TRUE;
    }
    let title = window_title(hwnd);
    let path = process_image_path(hwnd);
    let stem = exe_stem(&path);
    let looks_like_meet = stem.contains("wemeet")
        || stem.contains("tencentmeeting")
        || title.contains("腾讯会议")
        || title.to_ascii_lowercase().contains("tencent meeting");
    if looks_like_meet {
        data.handles.push(hwnd);
    }
    TRUE
}

fn split_rects(monitor: RECT) -> (RECT, RECT) {
    let width = monitor.right - monitor.left;
    let left_w = ((width as f64) * LEFT_RATIO).round() as i32;
    let left = RECT {
        left: monitor.left,
        top: monitor.top,
        right: monitor.left + left_w,
        bottom: monitor.bottom,
    };
    let right = RECT {
        left: monitor.left + left_w,
        top: monitor.top,
        right: monitor.right,
        bottom: monitor.bottom,
    };
    (left, right)
}

pub fn tile_with_tencent_meeting(app: &AppHandle, state: &GuardState) -> Result<String, String> {
    unsafe {
        let mut data = EnumData {
            handles: Vec::new(),
        };
        EnumWindows(Some(enum_callback), &mut data as *mut EnumData as isize);

        if data.handles.is_empty() {
            return Err("未找到腾讯会议窗口，请先打开并进入会议".into());
        }

        let monitor = primary_monitor_rect();
        let (left_rect, right_rect) = split_rects(monitor);
        let meet = data.handles[0];

        let Some(main) = app.get_window("main") else {
            return Err("主窗口未就绪".into());
        };
        let Ok(hwnd_main_raw) = main.hwnd() else {
            return Err("无法获取主窗口句柄".into());
        };
        let hwnd_main = hwnd_main_raw.0 as isize as HWND;

        // Full-height dual cover + topmost + lock against drag.
        force_pane(hwnd_main, left_rect, true);
        force_pane(meet, right_rect, true);
        let _ = main.set_always_on_top(true);

        {
            let mut g = state.inner.lock();
            g.topmost = true;
            g.layout_locked = true;
            g.main_pane = Some(LockedPane {
                hwnd: hwnd_main as isize,
                rect: left_rect,
            });
            g.meet_pane = Some(LockedPane {
                hwnd: meet as isize,
                rect: right_rect,
            });
        }

        let _ = app.emit("guard://status", &state.status());

        Ok(format!(
            "已锁定分屏并置顶：铺满主屏高度，禁止拖动下移（会议窗 {} 个）",
            data.handles.len()
        ))
    }
}

pub fn set_main_topmost(app: &AppHandle, state: &GuardState, topmost: bool) -> Result<(), String> {
    let Some(main) = app.get_window("main") else {
        return Err("主窗口未就绪".into());
    };
    main.set_always_on_top(topmost)
        .map_err(|e| e.to_string())?;

    let (main_pane, meet_pane) = {
        let mut g = state.inner.lock();
        g.topmost = topmost;
        // Turning topmost on while already tiled keeps lock; turning off releases lock
        // so the user can move windows again.
        if !topmost {
            g.layout_locked = false;
        } else if g.main_pane.is_some() && g.meet_pane.is_some() {
            g.layout_locked = true;
        }
        (g.main_pane, g.meet_pane)
    };

    unsafe {
        if let Some(pane) = main_pane {
            force_pane(pane.hwnd as HWND, pane.rect, topmost);
        }
        if let Some(pane) = meet_pane {
            force_pane(pane.hwnd as HWND, pane.rect, topmost);
        }
        // If unlocking and no stored panes, still clear topmost on main hwnd.
        if !topmost {
            if let Ok(hwnd_main) = main.hwnd() {
                SetWindowPos(
                    hwnd_main.0 as isize as HWND,
                    HWND_NOTOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOSIZE | SWP_NOMOVE,
                );
            }
        }
    }

    let _ = app.emit("guard://status", &state.status());
    Ok(())
}
