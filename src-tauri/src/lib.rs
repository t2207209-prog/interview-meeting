mod guard;
mod launch;

use tauri::{
    webview::{NewWindowResponse, WebviewBuilder},
    LogicalPosition, LogicalSize, Manager, WebviewUrl, WindowEvent,
};
use url::Url;

const DEFAULT_JOIN_URL: &str =
    "https://ms.symsun.com/interview3s-web/views/interview/meeting/meeting_join.html?token=inv-c507cc8aca8a448d";

const CHROME_H: f64 = 84.0;

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
fn tile_with_wemeet(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(windows)]
    {
        guard::tile_with_tencent_meeting(&app)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("窗口分屏仅支持 Windows".into())
    }
}

#[tauri::command]
fn set_main_topmost(app: tauri::AppHandle, topmost: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        guard::set_main_topmost(&app, topmost)
    }
    #[cfg(not(windows))]
    {
        if let Some(main) = app.get_window("main") {
            main.set_always_on_top(topmost).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(guard_state.clone())
        .invoke_handler(tauri::generate_handler![
            get_guard_status,
            set_guard_enabled,
            set_pull_back,
            tile_with_wemeet,
            set_main_topmost,
            open_meeting_link
        ])
        .setup(move |app| {
            let width = 1180.0;
            let height = 860.0;
            let join_url = Url::parse(DEFAULT_JOIN_URL).expect("valid join url");

            let window = tauri::window::WindowBuilder::new(app, "main")
                .title("Interview3S")
                .inner_size(width, height)
                .min_inner_size(900.0, 640.0)
                .build()?;

            let chrome_h = CHROME_H;
            let biz_h = height - chrome_h;

            let _chrome = window.add_child(
                WebviewBuilder::new("chrome", WebviewUrl::App("guard.html".into())),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(width, chrome_h),
            )?;

            let _biz = window.add_child(
                build_biz_webview(join_url),
                LogicalPosition::new(0.0, chrome_h),
                LogicalSize::new(width, biz_h),
            )?;

            let window_for_events = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::Resized(size) = event {
                    let scale = window_for_events.scale_factor().unwrap_or(1.0);
                    let logical = size.to_logical::<f64>(scale);
                    layout_main(&window_for_events, logical.width, logical.height);
                }
            });

            #[cfg(windows)]
            {
                guard::start_guard_loop(app.handle().clone(), guard_state.clone());
                guard_state.set_enabled(true);
                guard_state.set_pull_back(false);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
