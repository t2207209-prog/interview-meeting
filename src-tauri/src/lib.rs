use std::sync::OnceLock;
use tauri::{
    webview::{PermissionKind, PermissionResponse, WebviewBuilder},
    AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WindowEvent,
};
use url::Url;

const LEFT_RATIO: f64 = 0.38;

/// Only hide pure download banners. Do NOT hide guide/join-from-browser modals —
/// those often contain the only path into the web meeting room.
const MEET_INJECT_CSS: &str = r#"
  .download-banner,
  [class*="DownloadAppBanner"],
  [class*="download-app-banner"] {
    display: none !important;
  }
"#;

const MEET_INJECT_JS: &str = r#"
(function () {
  if (window.__imMeetHookInstalled) return;
  window.__imMeetHookInstalled = true;

  const WEB_JOIN_TEXTS = [
    '浏览器中入会',
    '浏览器入会',
    '从浏览器加入',
    '网页入会',
    '直接进入',
    '继续在浏览器',
    'Join from browser',
    'Join in browser',
    'Join From Browser'
  ];

  function digitsOnly(s) {
    return String(s || '').replace(/\D/g, '');
  }

  function meetingCodeFromHref(href) {
    try {
      const u = new URL(href, location.href);
      return (
        u.searchParams.get('meeting_code') ||
        u.searchParams.get('meetingCode') ||
        u.searchParams.get('code') ||
        ''
      );
    } catch (_) {
      const m = String(href).match(/meeting_code=([^&]+)/i);
      return m ? decodeURIComponent(m[1]) : '';
    }
  }

  function webJoinUrl(code) {
    const id = digitsOnly(code);
    if (!id) return 'https://meeting.tencent.com/user-center/join';
    return 'https://meeting.tencent.com/user-center/join?meeting_code=' + encodeURIComponent(id);
  }

  function clickBrowserJoinButtons() {
    const nodes = Array.from(document.querySelectorAll('a, button, [role="button"], span, div'));
    for (const el of nodes) {
      const t = (el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim();
      if (!t || t.length > 40) continue;
      if (WEB_JOIN_TEXTS.some((x) => t.includes(x))) {
        try { el.click(); return t; } catch (_) {}
      }
    }
    return null;
  }

  function redirectToWebJoin(code) {
    const clicked = clickBrowserJoinButtons();
    if (clicked) return;
    const url = webJoinUrl(code);
    if (location.href.indexOf('/user-center/join') === -1) {
      location.href = url;
    }
  }

  function handleProtocolUrl(raw) {
    if (!raw || !/^wemeet(ios)?|^tencentmeeting/i.test(String(raw))) return false;
    redirectToWebJoin(meetingCodeFromHref(raw));
    return true;
  }

  document.addEventListener('click', function (e) {
    const a = e.target && e.target.closest ? e.target.closest('a[href]') : null;
    if (!a) return;
    const href = a.getAttribute('href') || '';
    if (handleProtocolUrl(href)) {
      e.preventDefault();
      e.stopPropagation();
    }
  }, true);

  const nativeOpen = window.open;
  window.open = function (url) {
    if (handleProtocolUrl(url)) return null;
    return nativeOpen.apply(window, arguments);
  };

  try {
    const desc = Object.getOwnPropertyDescriptor(Location.prototype, 'href');
    if (desc && desc.set) {
      Object.defineProperty(Location.prototype, 'href', {
        configurable: true,
        enumerable: desc.enumerable,
        get: desc.get,
        set: function (v) {
          if (handleProtocolUrl(v)) return;
          return desc.set.call(this, v);
        }
      });
    }
  } catch (_) {}

  const css = `__CSS__`;
  const injectCss = () => {
    if (document.getElementById('im-hide-download')) return;
    const style = document.createElement('style');
    style.id = 'im-hide-download';
    style.textContent = css;
    (document.head || document.documentElement).appendChild(style);
  };
  injectCss();
  document.addEventListener('DOMContentLoaded', injectCss);

  // After "加入会议", a fallback modal may appear; auto-pick browser join.
  const mo = new MutationObserver(() => {
    clickBrowserJoinButtons();
  });
  mo.observe(document.documentElement, { childList: true, subtree: true });
  setInterval(clickBrowserJoinButtons, 1200);

  window.__imForceWebJoin = redirectToWebJoin;
})();
"#;

static APP: OnceLock<AppHandle> = OnceLock::new();

fn sanitize_meeting_id(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 9 {
        Some(digits)
    } else {
        None
    }
}

fn meeting_url(meeting_id: &str) -> Result<Url, String> {
    // Landing page still works; web-join redirect happens when wemeet:// is blocked.
    Url::parse(&format!("https://meeting.tencent.com/p/{meeting_id}"))
        .map_err(|e| e.to_string())
}

fn web_join_url(meeting_id: &str) -> Result<Url, String> {
    Url::parse(&format!(
        "https://meeting.tencent.com/user-center/join?meeting_code={meeting_id}"
    ))
    .map_err(|e| e.to_string())
}

fn meeting_code_from_protocol(url: &Url) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == "meeting_code" || k == "meetingCode" || k == "code")
        .map(|(_, v)| sanitize_meeting_id(&v))
        .flatten()
}

fn is_native_meeting_protocol(url: &Url) -> bool {
    matches!(url.scheme(), "wemeet" | "wemeetios" | "tencentmeeting")
}

fn layout_split(window: &tauri::Window, width: f64, height: f64) {
    let left_w = (width * LEFT_RATIO).round().max(280.0);
    let right_w = (width - left_w).max(320.0);

    if let Some(biz) = window.get_webview("biz") {
        let _ = biz.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = biz.set_size(LogicalSize::new(left_w, height));
    }
    if let Some(meet) = window.get_webview("meet") {
        let _ = meet.set_position(LogicalPosition::new(left_w, 0.0));
        let _ = meet.set_size(LogicalSize::new(right_w, height));
    }
}

fn redirect_meet_to_web_join(code: &str) {
    let Some(app) = APP.get() else { return };
    let Ok(url) = web_join_url(code) else { return };
    let Some(meet) = app.get_webview("meet") else { return };
    let meet2 = meet.clone();
    let code2 = code.to_string();
    // Avoid re-entrancy inside navigation callback.
    std::thread::spawn(move || {
        let _ = meet2.eval(
            "try{window.__imForceWebJoin && window.__imForceWebJoin('');}catch(e){}",
        );
        let _ = meet2.navigate(url);
        let _ = code2;
    });
}

#[tauri::command]
fn join_meeting(app: AppHandle, meeting_id: String) -> Result<String, String> {
    let id = sanitize_meeting_id(&meeting_id).ok_or_else(|| {
        "会议号无效：请输入至少 9 位数字（可带横杠）".to_string()
    })?;
    // Prefer the web join entry (may require login) over /p/{id}, which mainly launches the native client.
    let url = web_join_url(&id).or_else(|_| meeting_url(&id))?;
    let meet = app
        .get_webview("meet")
        .ok_or_else(|| "右侧会议视图未就绪".to_string())?;
    meet.navigate(url).map_err(|e| e.to_string())?;
    Ok(format!("已打开网页入会页：{id}（若需登录请先登录）"))
}

#[tauri::command]
fn reload_meeting_home(app: AppHandle) -> Result<(), String> {
    let meet = app
        .get_webview("meet")
        .ok_or_else(|| "右侧会议视图未就绪".to_string())?;
    let url = Url::parse("https://meeting.tencent.com/").map_err(|e| e.to_string())?;
    meet.navigate(url).map_err(|e| e.to_string())
}

fn build_meet_webview(url: WebviewUrl) -> WebviewBuilder<tauri::Wry> {
    let script = MEET_INJECT_JS.replace("__CSS__", &MEET_INJECT_CSS.replace('`', "\\`"));

    WebviewBuilder::new("meet", url)
        .on_navigation(|nav_url| {
            if is_native_meeting_protocol(nav_url) {
                if let Some(code) = meeting_code_from_protocol(nav_url) {
                    redirect_meet_to_web_join(&code);
                } else if let Some(app) = APP.get() {
                    // No code in protocol URL — ask page script to click browser-join UI.
                    if let Some(meet) = app.get_webview("meet") {
                        let _ = meet.eval(
                            "try{window.__imForceWebJoin && window.__imForceWebJoin('');}catch(e){}",
                        );
                    }
                }
                return false;
            }
            true
        })
        .on_permission_request(|_webview, kind| match kind {
            PermissionKind::Camera
            | PermissionKind::Microphone
            | PermissionKind::DisplayCapture
            | PermissionKind::Autoplay => PermissionResponse::Allow,
            _ => PermissionResponse::Default,
        })
        .initialization_script(&script)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![join_meeting, reload_meeting_home])
        .setup(|app| {
            let _ = APP.set(app.handle().clone());

            let width = 1400.0;
            let height = 860.0;

            let window = tauri::window::WindowBuilder::new(app, "main")
                .title("Interview Meeting MVP")
                .inner_size(width, height)
                .min_inner_size(900.0, 600.0)
                .build()?;

            let left_w = (width * LEFT_RATIO).round();
            let right_w = width - left_w;

            let _biz = window.add_child(
                WebviewBuilder::new("biz", WebviewUrl::App("index.html".into())),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(left_w, height),
            )?;

            let meet_url = WebviewUrl::External(
                Url::parse("https://meeting.tencent.com/").expect("valid url"),
            );
            let _meet = window.add_child(
                build_meet_webview(meet_url),
                LogicalPosition::new(left_w, 0.0),
                LogicalSize::new(right_w, height),
            )?;

            let window_for_events = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::Resized(size) = event {
                    let scale = window_for_events.scale_factor().unwrap_or(1.0);
                    let logical = size.to_logical::<f64>(scale);
                    layout_split(&window_for_events, logical.width, logical.height);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
