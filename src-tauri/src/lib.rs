mod capture;
mod guard;
mod launch;
mod session;

use tauri::{
    webview::{NewWindowResponse, WebviewBuilder},
    Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WindowEvent,
};
use url::Url;

const DEFAULT_JOIN_URL: &str =
    "https://ms.symsun.com/interview3s-web/views/interview/meeting/meeting_join.html?token=inv-c507cc8aca8a448d";

const CHROME_H: f64 = 100.0;

fn layout_full_overlay(window: &tauri::Window, id: &str, width: f64, height: f64) {
    if let Some(wv) = window.get_webview(id) {
        let _ = wv.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = wv.set_size(LogicalSize::new(width, height));
    }
}

fn layout_warn(window: &tauri::Window, width: f64, height: f64) {
    layout_full_overlay(window, "warn", width, height);
}

fn layout_face(window: &tauri::Window, width: f64, height: f64) {
    layout_full_overlay(window, "face", width, height);
}

fn layout_main(window: &tauri::Window, width: f64, height: f64) {
    let chrome_h = CHROME_H.min(height * 0.35).max(64.0);
    let biz_h = (height - chrome_h).max(120.0);

    if let Some(chrome) = window.get_webview("chrome") {
        let _ = chrome.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = chrome.set_size(LogicalSize::new(width, chrome_h));
    }
    if let Some(biz) = window.get_webview("biz") {
        let _ = biz.set_position(LogicalPosition::new(0.0, chrome_h));
        let _ = biz.set_size(LogicalSize::new(width, biz_h));
    }
}

fn collapse_shell(window: &tauri::Window) {
    for id in ["chrome", "biz"] {
        if let Some(wv) = window.get_webview(id) {
            let _ = wv.set_position(LogicalPosition::new(-10_000.0, -10_000.0));
            let _ = wv.set_size(LogicalSize::new(1.0, 1.0));
        }
    }
}

fn hide_overlay(window: &tauri::Window, id: &str) {
    if let Some(wv) = window.get_webview(id) {
        let _ = wv.set_position(LogicalPosition::new(-10_000.0, -10_000.0));
        let _ = wv.set_size(LogicalSize::new(1.0, 1.0));
    }
}

fn hide_warn_webview(window: &tauri::Window) {
    hide_overlay(window, "warn");
}

fn hide_face_webview(window: &tauri::Window) {
    hide_overlay(window, "face");
}

fn layout_for_state(
    window: &tauri::Window,
    width: f64,
    height: f64,
    session: &session::SessionState,
) {
    if session.is_interview_ready() {
        layout_main(window, width, height);
        hide_warn_webview(window);
        hide_face_webview(window);
    } else if session.is_acknowledged() {
        layout_face(window, width, height);
        hide_warn_webview(window);
        collapse_shell(window);
    } else {
        layout_warn(window, width, height);
        hide_face_webview(window);
        collapse_shell(window);
    }
}

fn reveal_face_ui(app: &tauri::AppHandle) {
    let Some(window) = app.get_window("main") else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(size) = window.inner_size() else {
        return;
    };
    let logical = size.to_logical::<f64>(scale);
    hide_warn_webview(&window);
    collapse_shell(&window);
    layout_face(&window, logical.width, logical.height);
}

fn reveal_interview_ui(
    app: &tauri::AppHandle,
    guard_state: &guard::GuardState,
    capture_state: &capture::CaptureState,
    session: &session::SessionState,
) {
    let Some(window) = app.get_window("main") else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(size) = window.inner_size() else {
        return;
    };
    let logical = size.to_logical::<f64>(scale);
    layout_main(&window, logical.width, logical.height);
    hide_warn_webview(&window);
    hide_face_webview(&window);

    if session.try_start_interview() {
        #[cfg(windows)]
        {
            let g = guard_state.clone();
            let c = capture_state.clone();
            guard::start_guard_loop(app.clone(), g.clone());
            guard::start_layout_lock_loop(g.clone());
            guard::start_meet_auto_loop(app.clone(), g.clone());
            g.set_enabled(true);
            g.set_pull_back(true);
            c.set_enabled(true);
            // Never block UI thread on PNG encode.
            std::thread::spawn(move || {
                let _ = capture::capture_once(&c);
            });
        }
        #[cfg(not(windows))]
        {
            let _ = (guard_state, capture_state);
        }
    }

    let _ = app.emit("session://ready", true);
}

fn launch_meeting_url(url: &Url) {
    let parsed = url.clone();
    // Avoid blocking the webview navigation callback.
    std::thread::spawn(move || {
        match launch::launch_meeting(&parsed, None) {
            Ok(target) => eprintln!("[launch] opened {target}"),
            Err(e) => eprintln!("[launch] {e}"),
        }
    });
}

#[tauri::command]
fn get_guard_status(state: tauri::State<'_, guard::GuardState>) -> guard::GuardStatus {
    state.status()
}

#[tauri::command]
fn set_guard_enabled(enabled: bool, state: tauri::State<'_, guard::GuardState>) -> guard::GuardStatus {
    state.set_enabled(enabled);
    state.status()
}

#[tauri::command]
fn set_pull_back(enabled: bool, state: tauri::State<'_, guard::GuardState>) -> guard::GuardStatus {
    state.set_pull_back(enabled);
    state.status()
}

#[tauri::command]
fn tile_with_wemeet(
    app: tauri::AppHandle,
    state: tauri::State<'_, guard::GuardState>,
) -> Result<String, String> {
    #[cfg(windows)]
    {
        guard::tile_with_tencent_meeting(&app, &state)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state);
        Err("窗口分屏仅支持 Windows".into())
    }
}

#[tauri::command]
fn set_main_topmost(
    app: tauri::AppHandle,
    topmost: bool,
    state: tauri::State<'_, guard::GuardState>,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        guard::set_main_topmost(&app, &state, topmost)
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        if let Some(main) = app.get_window("main") {
            main.set_always_on_top(topmost).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[tauri::command]
fn get_capture_status(state: tauri::State<'_, capture::CaptureState>) -> capture::CaptureStatus {
    state.status()
}

#[tauri::command]
fn set_capture_enabled(
    enabled: bool,
    state: tauri::State<'_, capture::CaptureState>,
) -> capture::CaptureStatus {
    state.set_enabled(enabled);
    state.status()
}

#[tauri::command]
fn capture_now(state: tauri::State<'_, capture::CaptureState>) -> Result<capture::CaptureStatus, String> {
    capture::capture_once(&state)?;
    Ok(state.status())
}

#[tauri::command]
fn open_capture_dir(state: tauri::State<'_, capture::CaptureState>) -> Result<String, String> {
    capture::open_save_dir(&state)
}

#[tauri::command]
fn acknowledge_warning(
    app: tauri::AppHandle,
    session: tauri::State<'_, session::SessionState>,
) -> Result<(), String> {
    if !session.mark_acknowledged() {
        return Ok(());
    }

    // Return immediately so the warn webview invoke does not deadlock.
    // Next gate: face capture / liveness demo page.
    let app_bg = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        let app_main = app_bg.clone();
        if let Err(e) = app_bg.run_on_main_thread(move || {
            reveal_face_ui(&app_main);
        }) {
            eprintln!("[session] reveal face failed: {e}");
        }
    });

    Ok(())
}

#[tauri::command]
fn complete_face_verify(
    app: tauri::AppHandle,
    session: tauri::State<'_, session::SessionState>,
    guard_state: tauri::State<'_, guard::GuardState>,
    capture_state: tauri::State<'_, capture::CaptureState>,
) -> Result<(), String> {
    if !session.is_acknowledged() {
        return Err("请先确认监考声明".into());
    }
    if !session.mark_face_verified() {
        return Ok(());
    }

    let app_bg = app.clone();
    let g = (*guard_state).clone();
    let c = (*capture_state).clone();
    let s = (*session).clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        let app_main = app_bg.clone();
        let g2 = g.clone();
        let c2 = c.clone();
        let s2 = s.clone();
        if let Err(e) = app_bg.run_on_main_thread(move || {
            reveal_interview_ui(&app_main, &g2, &c2, &s2);
        }) {
            eprintln!("[session] reveal interview failed: {e}");
        }
    });

    Ok(())
}

/// Open a Tencent Meeting https / wemeet link via native protocol when possible.
#[tauri::command]
fn open_meeting_link(url: String, meeting_code: Option<String>) -> Result<String, String> {
    let parsed = Url::parse(&url).map_err(|e| e.to_string())?;
    if !(launch::is_meeting_protocol(&parsed) || launch::is_tencent_meeting_https(&parsed)) {
        // Allow raw meeting-code-only helper: treat as code hint with dummy https.
        if let Some(code) = meeting_code.as_deref() {
            let digits: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() >= 9 {
                let target = launch::wemeet_deep_link(&digits);
                launch::open_externally(&target)?;
                return Ok(format!("已拉起腾讯会议：{target}"));
            }
        }
        return Err("仅允许打开腾讯会议相关链接".into());
    }
    let target = launch::launch_meeting(&parsed, meeting_code.as_deref())?;
    Ok(format!("已拉起腾讯会议：{target}"))
}

fn build_biz_webview(join_url: Url) -> WebviewBuilder<tauri::Wry> {
    WebviewBuilder::new("biz", WebviewUrl::External(join_url))
        .on_navigation(|url| {
            if launch::should_launch_externally(url) {
                // Convert https://meeting.tencent.com/...?...meetingcode= to wemeet://
                launch_meeting_url(url);
                return false;
            }
            true
        })
        .on_new_window(|url, _features| {
            if launch::should_launch_externally(&url) {
                launch_meeting_url(&url);
            }
            NewWindowResponse::Deny
        })
        .initialization_script(
            r#"
            (function () {
              if (window.__imLaunchHook) return;
              window.__imLaunchHook = true;

              function digitsOnly(s) {
                return String(s || '').replace(/\D/g, '');
              }

              function pageMeetingCode() {
                // Prefer explicit fields from the interview page.
                const candidates = [];
                document.querySelectorAll('input, textarea, [data-meeting-code], code, span, div, p, li').forEach((el) => {
                  const attr = el.getAttribute && el.getAttribute('data-meeting-code');
                  if (attr) candidates.push(attr);
                  if ('value' in el && el.value) candidates.push(String(el.value));
                  const t = (el.textContent || '').trim();
                  if (t && t.length < 40) candidates.push(t);
                });
                // Also scan body for patterns like 429 107 694 / 429-107-694
                const body = (document.body && document.body.innerText) || '';
                const m = body.match(/\b(\d{3}[\s\-]*\d{3}[\s\-]*\d{3})\b/);
                if (m) candidates.unshift(m[1]);
                for (const c of candidates) {
                  const d = digitsOnly(c);
                  if (d.length >= 9 && d.length <= 12) return d;
                }
                return '';
              }

              function isMeetingHref(href) {
                if (!href) return false;
                if (/^(wemeet|wemeetios|tencentmeeting):/i.test(href)) return true;
                try {
                  const u = new URL(href, location.href);
                  const h = (u.hostname || '').toLowerCase();
                  return h === 'meeting.tencent.com'
                    || h.endsWith('.meeting.tencent.com')
                    || h === 'voovmeeting.com'
                    || h.endsWith('.voovmeeting.com');
                } catch (_) { return false; }
              }

              function toWemeet(href, codeHint) {
                if (/^(wemeet|wemeetios|tencentmeeting):/i.test(href)) return href;
                try {
                  const u = new URL(href, location.href);
                  const code = digitsOnly(
                    u.searchParams.get('meeting_code')
                    || u.searchParams.get('meetingCode')
                    || u.searchParams.get('meetingcode')
                    || u.searchParams.get('code')
                    || codeHint
                    || ''
                  );
                  if (code.length >= 9) {
                    return 'wemeet://page/inmeeting?meeting_code=' + code;
                  }
                } catch (_) {}
                return href;
              }

              function openMeeting(href) {
                const code = pageMeetingCode();
                const target = toWemeet(href, code);
                // Trigger on_navigation in the host; Rust converts/opens via ShellExecute.
                location.href = target;
              }

              document.addEventListener('click', function (e) {
                const a = e.target && e.target.closest ? e.target.closest('a[href]') : null;
                if (a) {
                  const href = a.getAttribute('href') || a.href || '';
                  if (isMeetingHref(href)) {
                    e.preventDefault();
                    e.stopPropagation();
                    openMeeting(href);
                    return;
                  }
                }
                // Buttons that look like join actions often wrap no <a>.
                const btn = e.target && e.target.closest
                  ? e.target.closest('button, [role="button"], .join-btn, a')
                  : null;
                if (!btn) return;
                const label = ((btn.innerText || btn.textContent || '') + ' ' + (btn.getAttribute('href') || '')).trim();
                if (/腾讯会议|进入会议|加入会议|Join Meeting|进入腾讯/i.test(label)) {
                  // Prefer visible exclusive link on page.
                  const linkEl = document.querySelector('input[readonly], textarea, [class*="join"] a, a[href*="meeting.tencent.com"]');
                  let href = '';
                  if (linkEl) {
                    href = linkEl.getAttribute('href') || linkEl.value || linkEl.textContent || '';
                  }
                  href = (href || '').trim();
                  if (!isMeetingHref(href)) {
                    const code = pageMeetingCode();
                    if (code) href = 'wemeet://page/inmeeting?meeting_code=' + code;
                  }
                  if (href) {
                    e.preventDefault();
                    e.stopPropagation();
                    openMeeting(href);
                  }
                }
              }, true);

              const nativeOpen = window.open;
              window.open = function (url) {
                if (typeof url === 'string' && isMeetingHref(url)) {
                  openMeeting(url);
                  return null;
                }
                return nativeOpen.apply(window, arguments);
              };
            })();
            "#,
        )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let guard_state = guard::GuardState::default();
    let capture_state = capture::CaptureState::default();
    let session_state = session::SessionState::default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(guard_state.clone())
        .manage(capture_state.clone())
        .manage(session_state.clone())
        .invoke_handler(tauri::generate_handler![
            get_guard_status,
            set_guard_enabled,
            set_pull_back,
            tile_with_wemeet,
            set_main_topmost,
            open_meeting_link,
            get_capture_status,
            set_capture_enabled,
            capture_now,
            open_capture_dir,
            acknowledge_warning,
            complete_face_verify
        ])
        .setup(move |app| {
            let width = 1180.0;
            let height = 860.0;
            let join_url = Url::parse(DEFAULT_JOIN_URL).expect("valid join url");
            let chrome_h = CHROME_H;
            let biz_h = (height - chrome_h).max(120.0);

            let window = tauri::window::WindowBuilder::new(app, "main")
                .title("Interview3S")
                .inner_size(width, height)
                .min_inner_size(900.0, 640.0)
                .build()?;

            // Pre-create shell under the warning page so acknowledge never add_child
            // from inside a webview invoke (that deadlocks / freezes "正在进入").
            let _chrome = window.add_child(
                WebviewBuilder::new("chrome", WebviewUrl::App("guard.html".into())),
                LogicalPosition::new(-10_000.0, -10_000.0),
                LogicalSize::new(width, chrome_h),
            )?;
            let _biz = window.add_child(
                build_biz_webview(join_url),
                LogicalPosition::new(-10_000.0, -10_000.0),
                LogicalSize::new(width, biz_h),
            )?;
            let _face = window.add_child(
                WebviewBuilder::new("face", WebviewUrl::App("face.html".into())),
                LogicalPosition::new(-10_000.0, -10_000.0),
                LogicalSize::new(width, height),
            )?;
            let _warn = window.add_child(
                WebviewBuilder::new("warn", WebviewUrl::App("warning.html".into())),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(width, height),
            )?;

            let window_for_events = window.clone();
            let session_for_events = session_state.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::Resized(size) = event {
                    let scale = window_for_events.scale_factor().unwrap_or(1.0);
                    let logical = size.to_logical::<f64>(scale);
                    layout_for_state(
                        &window_for_events,
                        logical.width,
                        logical.height,
                        &session_for_events,
                    );
                }
            });

            #[cfg(windows)]
            {
                let shots = app
                    .path()
                    .app_data_dir()
                    .map(|p| p.join("screenshots"))
                    .unwrap_or_else(|_| {
                        std::env::temp_dir().join("interview-meeting").join("screenshots")
                    });
                capture_state.init_dir(shots);
                capture_state.set_interval_secs(30);
                // Stay off until candidate acknowledges the warning.
                capture_state.set_enabled(false);
                capture::start_capture_loop(app.handle().clone(), capture_state.clone());
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
