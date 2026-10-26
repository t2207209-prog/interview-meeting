use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{
    webview::{PermissionKind, PermissionResponse, WebviewBuilder},
    AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WindowEvent,
};
use url::Url;

const LEFT_RATIO: f64 = 0.38;

static APP: OnceLock<AppHandle> = OnceLock::new();
/// When true, the next `wemeet://` navigation is allowed (opens native client once).
static ALLOW_NATIVE_ONCE: AtomicBool = AtomicBool::new(false);

const MEET_INJECT_JS: &str = r#"
(function () {
  if (window.__imMeetHookInstalled) return;
  window.__imMeetHookInstalled = true;

  const WEB_JOIN_TEXTS = [
    '浏览器中入会', '浏览器入会', '从浏览器加入', '网页入会',
    'Join from browser', 'Join in browser', 'Join From Browser'
  ];

  function digitsOnly(s) {
    return String(s || '').replace(/\D/g, '');
  }

  function meetingCodeFromText() {
    const m = document.body && document.body.innerText
      ? document.body.innerText.match(/\b(\d{3}\s*\d{3}\s*\d{3})\b/)
      : null;
    if (m) return digitsOnly(m[1]);
    try {
      const u = new URL(location.href);
      return digitsOnly(
        u.searchParams.get('meeting_code') ||
        u.searchParams.get('meetingCode') ||
        u.searchParams.get('meetingcode') ||
        u.pathname.split('/').pop() ||
        ''
      );
    } catch (_) { return ''; }
  }

  function meetingCodeFromHref(href) {
    try {
      const u = new URL(href, location.href);
      return digitsOnly(
        u.searchParams.get('meeting_code') ||
        u.searchParams.get('meetingCode') ||
        u.searchParams.get('code') ||
        ''
      );
    } catch (_) {
      const m = String(href).match(/meeting_code=([^&]+)/i);
      return m ? digitsOnly(decodeURIComponent(m[1])) : '';
    }
  }

  function clickBrowserJoinButtons() {
    const nodes = Array.from(document.querySelectorAll('a, button, [role="button"], span, div'));
    for (const el of nodes) {
      const t = (el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim();
      if (!t || t.length > 48) continue;
      if (WEB_JOIN_TEXTS.some((x) => t.includes(x))) {
        try { el.click(); return t; } catch (_) {}
      }
    }
    return null;
  }

  function isLaunchPage() {
    const t = (document.body && document.body.innerText) || '';
    return t.includes('启动中') || t.includes('正在启动腾讯会议');
  }

  function ensureOverlay() {
    if (document.getElementById('im-join-overlay')) return;
    if (!isLaunchPage()) return;

    const code = meetingCodeFromText();
    const box = document.createElement('div');
    box.id = 'im-join-overlay';
    box.style.cssText = [
      'position:fixed', 'inset:0', 'z-index:2147483647',
      'background:rgba(18,24,32,.55)', 'display:flex',
      'align-items:center', 'justify-content:center',
      'font-family:Segoe UI,PingFang SC,Microsoft YaHei,sans-serif'
    ].join(';');

    box.innerHTML = `
      <div style="width:min(440px,92vw);background:#fffdf8;border-radius:16px;padding:22px 20px;box-shadow:0 16px 40px rgba(0,0,0,.2);color:#1c2430">
        <div style="font-weight:700;font-size:18px;margin-bottom:8px">网页端在尝试打开客户端</div>
        <p style="margin:0 0 14px;color:#5b6573;font-size:14px;line-height:1.55">
          「加入」会先进入「启动中…」并用 <code>wemeet://</code> 唤起本机腾讯会议。
          本应用默认拦截该协议，所以会闪一下又回到入会页。
          中国大陆网页入会页目前几乎只会拉客户端，很难在内嵌窗口里直接开音视频会议。
        </p>
        <div style="display:flex;flex-direction:column;gap:10px">
          <button id="im-try-invite" style="appearance:none;border:0;border-radius:999px;padding:11px 14px;font:inherit;font-weight:600;cursor:pointer;background:#0f6e56;color:#f4fff9">
            打开邀请链接页再试（/p/会议号）
          </button>
          <button id="im-allow-native" style="appearance:none;border:1px solid #d7d0c4;border-radius:999px;padding:11px 14px;font:inherit;font-weight:600;cursor:pointer;background:#fff;color:#1c2430">
            允许唤起本机客户端（会跳出本窗口）
          </button>
          <button id="im-dismiss" style="appearance:none;border:0;background:transparent;color:#5b6573;padding:6px;cursor:pointer;font:inherit">
            关闭提示
          </button>
        </div>
      </div>`;
    document.documentElement.appendChild(box);

    document.getElementById('im-try-invite')?.addEventListener('click', () => {
      const id = code || meetingCodeFromText();
      if (id) location.href = 'https://meeting.tencent.com/p/' + id;
      else location.href = 'https://meeting.tencent.com/';
    });

    document.getElementById('im-allow-native')?.addEventListener('click', () => {
      // Signal Rust to allow the next wemeet:// once, then retry join via sentinel URL.
      location.href = 'https://meeting.tencent.com/?im_allow_native=1&meeting_code=' + encodeURIComponent(code || '');
    });

    document.getElementById('im-dismiss')?.addEventListener('click', () => {
      box.remove();
    });
  }

  function handleProtocolUrl(raw) {
    if (!raw || !/^wemeet(ios)?|^tencentmeeting/i.test(String(raw))) return false;
    // Do NOT bounce back to user-center/join — that causes the flash loop.
    setTimeout(() => {
      if (!clickBrowserJoinButtons()) ensureOverlay();
    }, 300);
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

  const tick = () => {
    clickBrowserJoinButtons();
    if (isLaunchPage()) ensureOverlay();
  };
  document.addEventListener('DOMContentLoaded', tick);
  new MutationObserver(tick).observe(document.documentElement, { childList: true, subtree: true });
  setInterval(tick, 1000);
})();
"#;

fn sanitize_meeting_id(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 9 {
        Some(digits)
    } else {
        None
    }
}

fn invite_url(meeting_id: &str) -> Result<Url, String> {
    Url::parse(&format!("https://meeting.tencent.com/p/{meeting_id}")).map_err(|e| e.to_string())
}

fn center_join_url(meeting_id: &str) -> Result<Url, String> {
    Url::parse(&format!(
        "https://meeting.tencent.com/user-center/join?meeting_code={meeting_id}"
    ))
    .map_err(|e| e.to_string())
}

fn meeting_code_from_url(url: &Url) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| {
            matches!(
                k.as_ref(),
                "meeting_code" | "meetingCode" | "meetingcode" | "code"
            )
        })
        .map(|(_, v)| sanitize_meeting_id(&v))
        .flatten()
}

fn is_native_meeting_protocol(url: &Url) -> bool {
    matches!(url.scheme(), "wemeet" | "wemeetios" | "tencentmeeting")
}

fn wants_allow_native(url: &Url) -> bool {
    url.query_pairs().any(|(k, v)| k == "im_allow_native" && v == "1")
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

fn show_launch_overlay_in_meet() {
    let Some(app) = APP.get() else { return };
    let Some(meet) = app.get_webview("meet") else { return };
    let _ = meet.eval(
        r#"
        try {
          const t = (document.body && document.body.innerText) || '';
          if (t.includes('启动中') || t.includes('正在启动腾讯会议')) {
            // force overlay helper from injected script
            const ev = new Event('DOMContentLoaded');
            document.dispatchEvent(ev);
          }
        } catch (e) {}
        "#,
    );
}

#[tauri::command]
fn join_meeting(app: AppHandle, meeting_id: String) -> Result<String, String> {
    let id = sanitize_meeting_id(&meeting_id).ok_or_else(|| {
        "会议号无效：请输入至少 9 位数字（可带横杠）".to_string()
    })?;
    let url = invite_url(&id)?;
    let meet = app
        .get_webview("meet")
        .ok_or_else(|| "右侧会议视图未就绪".to_string())?;
    meet.navigate(url).map_err(|e| e.to_string())?;
    Ok(format!("已打开邀请链接页 /p/{id}"))
}

#[tauri::command]
fn join_meeting_center(app: AppHandle, meeting_id: String) -> Result<String, String> {
    let id = sanitize_meeting_id(&meeting_id).ok_or_else(|| {
        "会议号无效：请输入至少 9 位数字（可带横杠）".to_string()
    })?;
    let url = center_join_url(&id)?;
    let meet = app
        .get_webview("meet")
        .ok_or_else(|| "右侧会议视图未就绪".to_string())?;
    meet.navigate(url).map_err(|e| e.to_string())?;
    Ok(format!("已打开用户中心入会页（通常会唤起客户端）：{id}"))
}

#[tauri::command]
fn allow_native_client_once() -> Result<String, String> {
    ALLOW_NATIVE_ONCE.store(true, Ordering::SeqCst);
    Ok("已允许下一次唤起本机腾讯会议客户端。请在右侧再点「加入」。".into())
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
    WebviewBuilder::new("meet", url)
        .on_navigation(|nav_url| {
            // Left-panel / overlay asked to allow native client once.
            if wants_allow_native(nav_url) {
                ALLOW_NATIVE_ONCE.store(true, Ordering::SeqCst);
                if let Some(code) = meeting_code_from_url(nav_url) {
                    // Return to center join so user can click 加入 again with allow flag set.
                    if let Some(app) = APP.get() {
                        if let Some(meet) = app.get_webview("meet") {
                            if let Ok(u) = center_join_url(&code) {
                                let meet2 = meet.clone();
                                std::thread::spawn(move || {
                                    let _ = meet2.navigate(u);
                                });
                            }
                        }
                    }
                }
                return false;
            }

            if is_native_meeting_protocol(nav_url) {
                if ALLOW_NATIVE_ONCE.swap(false, Ordering::SeqCst) {
                    // Let Windows open the installed Tencent Meeting client.
                    return true;
                }
                // Block protocol and stay on current page (avoid join-page bounce loop).
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    show_launch_overlay_in_meet();
                });
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
        .initialization_script(MEET_INJECT_JS)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            join_meeting,
            join_meeting_center,
            allow_native_client_once,
            reload_meeting_home
        ])
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
