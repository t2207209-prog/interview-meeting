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
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, MoveWindow,
    SetForegroundWindow, SetWindowPos, ShowWindow, GW_OWNER, HWND_NOTOPMOST, HWND_TOPMOST,
    SW_RESTORE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
};

const POLL_MS: u64 = 350;
const LAYOUT_POLL_MS: u64 = 200;
const MEET_AUTO_POLL_MS: u64 = 800;
const MEET_APPEAR_DEBOUNCE_MS: u64 = 1200;
/// Ignore tiny splash / tray popups when auto-detecting the main meet window.
const MEET_MIN_W: i32 = 480;
const MEET_MIN_H: i32 = 320;
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

fn is_meet_stem(stem: &str) -> bool {
    stem.contains("wemeet")
        || stem.contains("tencentmeeting")
        || stem.contains("wechatmeeting")
}

unsafe fn promote_topmost_only(hwnd: HWND) {
    if hwnd.is_null() || IsWindow(hwnd) == 0 {
        return;
    }
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOSIZE | SWP_NOMOVE,
    );
}

/// While layout is locked, neither pane may stay minimized.
unsafe fn ensure_not_minimized(hwnd: HWND) -> bool {
    if hwnd.is_null() || IsWindow(hwnd) == 0 {
        return false;
    }
    if IsIconic(hwnd) != 0 {
        ShowWindow(hwnd, SW_RESTORE);
        true
    } else {
        false
    }
}

/// True when foreground is a Tencent Meeting window other than the locked main meet pane
/// (settings / permission / confirm dialogs, etc.).
unsafe fn active_meet_popup(main_meet: HWND) -> Option<HWND> {
    let fg = GetForegroundWindow();
    if fg.is_null() || fg == main_meet {
        return None;
    }
    let stem = exe_stem(&process_image_path(fg));
    if is_meet_stem(&stem) {
        Some(fg)
    } else {
        None
    }
}

/// Continuously snap panes while locked.
/// - Our app: always force left full-height rect.
/// - Tencent Meeting: allow its own popups; only soft-snap the main meet window when idle.
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
            // 1) Always keep our interview window fixed (covers left desktop).
            if let Some(pane) = main {
                let hwnd = pane.hwnd as HWND;
                if IsWindow(hwnd) != 0 {
                    let was_min = ensure_not_minimized(hwnd);
                    let drifted = current_rect(hwnd)
                        .map(|r| !rect_eq(&r, &pane.rect))
                        .unwrap_or(true);
                    if was_min || drifted {
                        force_pane(hwnd, pane.rect, topmost);
                    } else if topmost {
                        promote_topmost_only(hwnd);
                    }
                }
            }

            // 2) Tencent Meeting: pass-through for its dialogs / child windows.
            if let Some(pane) = meet {
                let hwnd = pane.hwnd as HWND;
                if IsWindow(hwnd) == 0 {
                    continue;
                }

                // Never allow the main meet pane to stay minimized while locked.
                let was_min = ensure_not_minimized(hwnd);

                if let Some(popup) = active_meet_popup(hwnd) {
                    // Let the popup show above everything; do NOT MoveWindow the main meet frame.
                    promote_topmost_only(popup);
                    continue;
                }

                // Soft-snap only the main meeting window if minimized or dragged away.
                let drifted = current_rect(hwnd)
                    .map(|r| !rect_eq(&r, &pane.rect))
                    .unwrap_or(false);
                if was_min || drifted {
                    force_pane(hwnd, pane.rect, topmost);
                } else if topmost {
                    promote_topmost_only(hwnd);
                }
            }
        }
    });
}

struct EnumData {
    handles: Vec<HWND>,
}

unsafe fn looks_like_meet_window(hwnd: HWND) -> bool {
    let title = window_title(hwnd);
    let path = process_image_path(hwnd);
    let stem = exe_stem(&path);
    is_meet_stem(&stem)
        || title.contains("腾讯会议")
        || title.to_ascii_lowercase().contains("tencent meeting")
}

/// Home / parent client of Tencent Meeting (not the concrete in-meeting room).
fn is_meet_shell_title(title: &str) -> bool {
    let t = title.trim();
    t.is_empty()
        || t == "腾讯会议"
        || t == "WeMeet"
        || t.eq_ignore_ascii_case("Tencent Meeting")
}

unsafe fn window_area(hwnd: HWND) -> i64 {
    current_rect(hwnd)
        .map(|r| (r.right as i64 - r.left as i64) * (r.bottom as i64 - r.top as i64))
        .unwrap_or(0)
}

/// Prefer the concrete in-meeting window over the parent/home client.
unsafe fn meet_pick_score(hwnd: HWND, z_bonus: i64) -> i64 {
    let title = window_title(hwnd);
    let area = window_area(hwnd);
    let mut score = area + z_bonus;
    if is_meet_shell_title(&title) {
        // Heavy penalty so any titled meeting room wins over the home shell.
        score -= 1_000_000_000;
    } else {
        score += 200_000_000;
        score += (title.chars().count() as i64).min(64) * 20_000;
    }
    // Owned frames are often the in-meeting surface relative to the shell.
    let owner = GetWindow(hwnd, GW_OWNER);
    if !owner.is_null() {
        score += 80_000_000;
    }
    score
}

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let data = &mut *(lparam as *mut EnumData);
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
        return TRUE;
    }
    if !looks_like_meet_window(hwnd) {
        return TRUE;
    }
    if let Some(r) = current_rect(hwnd) {
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        // Skip tiny splash / notification chrome.
        if w < MEET_MIN_W || h < MEET_MIN_H {
            return TRUE;
        }
    }
    data.handles.push(hwnd);
    TRUE
}

unsafe fn find_meet_windows() -> Vec<HWND> {
    let mut data = EnumData {
        handles: Vec::new(),
    };
    EnumWindows(Some(enum_callback), &mut data as *mut EnumData as isize);
    data.handles
}

/// Pick the concrete in-meeting window. Returns None when only the parent/home client is open
/// so we do not tile/topmost against the shell alone.
unsafe fn pick_main_meet(handles: &[HWND]) -> Option<HWND> {
    if handles.is_empty() {
        return None;
    }

    let has_room = handles
        .iter()
        .any(|&h| !is_meet_shell_title(&window_title(h)));

    // Only the parent client — wait until a real meeting room appears.
    if !has_room && handles.len() < 2 {
        return None;
    }

    // All titles look like the shell but multiple large windows exist: prefer the topmost
    // (EnumWindows is z-order, front first) that is not uniquely the biggest home frame.
    // Score still breaks ties by area + ownership.
    let mut best: Option<(HWND, i64)> = None;
    let n = handles.len() as i64;
    for (i, &hwnd) in handles.iter().enumerate() {
        // When only shell-titled windows exist, demote the single largest (usually home).
        let mut score = meet_pick_score(hwnd, (n - i as i64) * 5_000);
        if !has_room {
            let max_area = handles.iter().map(|&h| window_area(h)).max().unwrap_or(0);
            if window_area(hwnd) == max_area {
                score -= 50_000_000;
            }
        }
        if best.map(|(_, s)| score > s).unwrap_or(true) {
            best = Some((hwnd, score));
        }
    }
    best.map(|(h, _)| h)
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
        let handles = find_meet_windows();
        let Some(meet) = pick_main_meet(&handles) else {
            return Err("未找到具体会议窗口（仅父客户端不会并排），请先进入会议".into());
        };

        let monitor = primary_monitor_rect();
        let (left_rect, right_rect) = split_rects(monitor);

        let Some(main) = app.get_window("main") else {
            return Err("主窗口未就绪".into());
        };
        let Ok(hwnd_main_raw) = main.hwnd() else {
            return Err("无法获取主窗口句柄".into());
        };
        let hwnd_main = hwnd_main_raw.0 as isize as HWND;

        // Full-height dual cover + topmost + lock against drag / minimize.
        ensure_not_minimized(hwnd_main);
        ensure_not_minimized(meet);
        force_pane(hwnd_main, left_rect, true);
        force_pane(meet, right_rect, true);
        let _ = main.set_always_on_top(true);
        let _ = main.set_minimizable(false);

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
            "已锁定分屏并置顶：铺满主屏、禁止拖动/最小化（会议窗 {} 个）",
            handles.len()
        ))
    }
}

/// Clear lock / topmost (used when Tencent Meeting exits).
pub fn release_layout_lock(app: &AppHandle, state: &GuardState) -> Result<(), String> {
    let Some(main) = app.get_window("main") else {
        return Err("主窗口未就绪".into());
    };
    let _ = main.set_always_on_top(false);
    let _ = main.set_minimizable(true);

    let (main_pane, meet_pane) = {
        let mut g = state.inner.lock();
        g.topmost = false;
        g.layout_locked = false;
        (g.main_pane.take(), g.meet_pane.take())
    };

    unsafe {
        let clear_topmost = |hwnd: HWND| {
            if hwnd.is_null() || IsWindow(hwnd) == 0 {
                return;
            }
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOSIZE | SWP_NOMOVE,
            );
        };
        if let Some(pane) = main_pane {
            clear_topmost(pane.hwnd as HWND);
        }
        if let Some(pane) = meet_pane {
            clear_topmost(pane.hwnd as HWND);
        }
        if let Ok(hwnd_main) = main.hwnd() {
            clear_topmost(hwnd_main.0 as isize as HWND);
        }
    }

    let _ = app.emit("guard://status", &state.status());
    Ok(())
}

/// Watch Tencent Meeting: tile+topmost only with a concrete meeting room window;
/// release topmost when that room is gone (parent/home alone does not keep us topmost).
pub fn start_meet_auto_loop(app: AppHandle, state: GuardState) {
    thread::spawn(move || {
        let mut was_in_meeting = false;
        loop {
            thread::sleep(Duration::from_millis(MEET_AUTO_POLL_MS));

            let handles = unsafe { find_meet_windows() };
            let preferred = unsafe { pick_main_meet(&handles) };
            let in_meeting = preferred.is_some();
            let (locked, meet_hwnd) = {
                let g = state.inner.lock();
                (g.layout_locked, g.meet_pane.map(|p| p.hwnd))
            };

            if let Some(target) = preferred {
                let target_id = target as isize;
                let dead = meet_hwnd
                    .map(|h| unsafe { IsWindow(h as HWND) == 0 })
                    .unwrap_or(true);
                let switched = meet_hwnd.map(|h| h != target_id).unwrap_or(true);
                let need_tile = !locked || dead || switched;

                if need_tile {
                    if !was_in_meeting {
                        // Wait for the real in-meeting frame to settle, then tile.
                        thread::sleep(Duration::from_millis(MEET_APPEAR_DEBOUNCE_MS));
                        let handles2 = unsafe { find_meet_windows() };
                        if unsafe { pick_main_meet(&handles2) }.is_none() {
                            was_in_meeting = false;
                            continue;
                        }
                    }
                    match tile_with_tencent_meeting(&app, &state) {
                        Ok(msg) => {
                            eprintln!("[meet-auto] {msg}");
                            let _ = app.emit("guard://auto-tiled", state.status());
                        }
                        Err(e) => eprintln!("[meet-auto] tile skipped: {e}"),
                    }
                }
                was_in_meeting = true;
            } else if was_in_meeting || locked {
                // No concrete meeting room (or only parent client) → drop topmost/lock.
                match release_layout_lock(&app, &state) {
                    Ok(()) => {
                        eprintln!("[meet-auto] no in-meeting window → unlocked topmost");
                        let _ = app.emit("guard://auto-released", state.status());
                    }
                    Err(e) => eprintln!("[meet-auto] release failed: {e}"),
                }
                was_in_meeting = false;
            } else {
                was_in_meeting = false;
            }
        }
    });
}

pub fn set_main_topmost(app: &AppHandle, state: &GuardState, topmost: bool) -> Result<(), String> {
    let Some(main) = app.get_window("main") else {
        return Err("主窗口未就绪".into());
    };
    main.set_always_on_top(topmost)
        .map_err(|e| e.to_string())?;

    let (main_pane, meet_pane, locked) = {
        let mut g = state.inner.lock();
        g.topmost = topmost;
        // Turning topmost off releases lock so the user can move / minimize again.
        if !topmost {
            g.layout_locked = false;
        } else if g.main_pane.is_some() && g.meet_pane.is_some() {
            g.layout_locked = true;
        }
        (g.main_pane, g.meet_pane, g.layout_locked)
    };
    let _ = main.set_minimizable(!locked);

    unsafe {
        if let Some(pane) = main_pane {
            if topmost {
                ensure_not_minimized(pane.hwnd as HWND);
            }
            force_pane(pane.hwnd as HWND, pane.rect, topmost);
        }
        if let Some(pane) = meet_pane {
            if topmost {
                ensure_not_minimized(pane.hwnd as HWND);
            }
            if IsWindow(pane.hwnd as HWND) != 0 {
                force_pane(pane.hwnd as HWND, pane.rect, topmost);
            }
        }
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
