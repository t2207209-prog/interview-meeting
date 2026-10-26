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
    EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, MoveWindow, SetForegroundWindow, SetWindowPos, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
};

const POLL_MS: u64 = 350;

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
}

#[derive(Default)]
struct GuardInner {
    enabled: bool,
    pull_back: bool,
    violations: u32,
    last_process: String,
    last_title: String,
    foreground_allowed: bool,
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

fn primary_work_area() -> RECT {
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
        info.rcWork
    }
}

pub fn tile_with_tencent_meeting(app: &AppHandle) -> Result<String, String> {
    unsafe {
        let mut data = EnumData {
            handles: Vec::new(),
        };
        EnumWindows(Some(enum_callback), &mut data as *mut EnumData as isize);

        if data.handles.is_empty() {
            return Err("未找到腾讯会议窗口，请先打开并进入会议".into());
        }

        let work = primary_work_area();
        let width = work.right - work.left;
        let height = work.bottom - work.top;
        let left_w = (width as f64 * 0.58).round() as i32;
        let right_w = width - left_w;

        let meet = data.handles[0];
        MoveWindow(meet, work.left + left_w, work.top, right_w, height, 1);
        SetWindowPos(
            meet,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOSIZE | SWP_NOMOVE,
        );

        if let Some(main) = app.get_window("main") {
            let _ = main.set_always_on_top(true);
            let scale = main.scale_factor().unwrap_or(1.0);
            let _ = main.set_position(tauri::LogicalPosition::new(
                (work.left as f64) / scale,
                (work.top as f64) / scale,
            ));
            let _ = main.set_size(tauri::LogicalSize::new(
                (left_w as f64) / scale,
                (height as f64) / scale,
            ));
            if let Ok(hwnd_main) = main.hwnd() {
                SetWindowPos(
                    hwnd_main.0 as isize as HWND,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOSIZE | SWP_NOMOVE,
                );
            }
        }

        Ok(format!(
            "已分屏：左侧本应用约 58%，右侧腾讯会议约 42%（找到 {} 个会议窗口）",
            data.handles.len()
        ))
    }
}

pub fn set_main_topmost(app: &AppHandle, topmost: bool) -> Result<(), String> {
    if let Some(main) = app.get_window("main") {
        main.set_always_on_top(topmost)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
