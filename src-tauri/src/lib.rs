use tauri::{
    webview::{PermissionKind, PermissionResponse, WebviewBuilder},
    AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WindowEvent,
};
use url::Url;

const LEFT_RATIO: f64 = 0.38;
const MEET_INJECT_CSS: &str = r#"
  .download-banner,
  .guide-download-modal,
  [class*="download-banner"],
  [class*="DownloadBanner"],
  [class*="guide-download"] {
    display: none !important;
  }
"#;

fn sanitize_meeting_id(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 9 {
        Some(digits)
    } else {
        None
    }
}

fn meeting_url(meeting_id: &str) -> Result<Url, String> {
    Url::parse(&format!("https://meeting.tencent.com/p/{meeting_id}"))
        .map_err(|e| e.to_string())
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

#[tauri::command]
fn join_meeting(app: AppHandle, meeting_id: String) -> Result<String, String> {
    let id = sanitize_meeting_id(&meeting_id).ok_or_else(|| {
        "会议号无效：请输入至少 9 位数字（可带横杠）".to_string()
    })?;
    let url = meeting_url(&id)?;
    let meet = app
        .get_webview("meet")
        .ok_or_else(|| "右侧会议视图未就绪".to_string())?;
    meet.navigate(url).map_err(|e| e.to_string())?;
    Ok(format!("已加载会议页：{id}"))
}

#[tauri::command]
fn reload_meeting_home(app: AppHandle) -> Result<(), String> {
    let meet = app
        .get_webview("meet")
        .ok_or_else(|| "右侧会议视图未就绪".to_string())?;
    let url = Url::parse("https://meeting.tencent.com/").map_err(|e| e.to_string())?;
    meet.navigate(url).map_err(|e| e.to_string())
}

fn build_meet_webview(url: WebviewUrl) -> WebviewBuilder {
    WebviewBuilder::new("meet", url)
        .on_navigation(|nav_url| {
            let scheme = nav_url.scheme();
            // Block protocol launches that open the native Tencent Meeting client.
            if scheme == "wemeet" || scheme == "wemeetios" || scheme == "tencentmeeting" {
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
        .initialization_script(&format!(
            r#"
            (function () {{
              const css = `{css}`;
              const inject = () => {{
                if (document.getElementById('im-hide-download')) return;
                const style = document.createElement('style');
                style.id = 'im-hide-download';
                style.textContent = css;
                (document.head || document.documentElement).appendChild(style);
              }};
              inject();
              document.addEventListener('DOMContentLoaded', inject);
              new MutationObserver(inject).observe(document.documentElement, {{
                childList: true,
                subtree: true
              }});
            }})();
            "#,
            css = MEET_INJECT_CSS.replace('`', "\\`")
        ))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![join_meeting, reload_meeting_home])
        .setup(|app| {
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
