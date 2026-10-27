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
    let s = url.as_str().to_string();
    // Avoid blocking the webview navigation callback.
    std::thread::spawn(move || {
        if let Err(e) = launch::open_externally(&s) {
            eprintln!("[launch] {e}");
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

/// Open a Tencent Meeting https / wemeet link via the OS (browser or protocol handler).
#[tauri::command]
fn open_meeting_link(url: String) -> Result<String, String> {
    let parsed = Url::parse(&url).map_err(|e| e.to_string())?;
    if !(launch::is_meeting_protocol(&parsed) || launch::is_tencent_meeting_https(&parsed)) {
        return Err("仅允许打开腾讯会议相关链接".into());
    }
    launch::open_externally(parsed.as_str())?;
    Ok(format!("已请求系统打开：{}", parsed))
}

fn build_biz_webview(join_url: Url) -> WebviewBuilder<tauri::Wry> {
    WebviewBuilder::new("biz", WebviewUrl::External(join_url))
        .on_navigation(|url| {
            if launch::should_launch_externally(url) {
                launch_meeting_url(url);
                // Keep the interview page in the webview; OS handles meeting launch.
                return false;
            }
            true
        })
        .on_new_window(|url, _features| {
            if launch::should_launch_externally(&url) {
                launch_meeting_url(&url);
            } else if matches!(url.scheme(), "http" | "https") {
                // Other blank targets: also open externally to avoid nested windows.
                let s = url.as_str().to_string();
                std::thread::spawn(move || {
                    let _ = launch::open_externally(&s);
                });
            }
            NewWindowResponse::Deny
        })
        .initialization_script(
            r#"
            (function () {
              if (window.__imLaunchHook) return;
              window.__imLaunchHook = true;

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

              function openExternal(href) {
                // Prefer absolute URL
                try {
                  const abs = new URL(href, location.href).href;
                  // Custom protocol for Rust side may not be available on remote pages;
                  // also set location as fallback trigger for on_navigation.
                  location.href = abs;
                } catch (_) {}
              }

              document.addEventListener('click', function (e) {
                const a = e.target && e.target.closest ? e.target.closest('a[href]') : null;
                if (!a) return;
                const href = a.getAttribute('href') || a.href || '';
                if (!isMeetingHref(href)) return;
                e.preventDefault();
                e.stopPropagation();
                openExternal(href);
              }, true);

              const nativeOpen = window.open;
              window.open = function (url) {
                if (typeof url === 'string' && isMeetingHref(url)) {
                  openExternal(url);
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
