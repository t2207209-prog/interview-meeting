mod guard;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

/// Default remote join page for this MVP.
const DEFAULT_JOIN_URL: &str =
    "https://ms.symsun.com/interview3s-web/views/interview/meeting/meeting_join.html?token=inv-c507cc8aca8a448d";

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
        if let Some(main) = app.get_webview_window("main") {
            main.set_always_on_top(topmost).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
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
            set_main_topmost
        ])
        .setup(move |app| {
            let join_url = Url::parse(DEFAULT_JOIN_URL).expect("valid join url");

            let main = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(join_url),
            )
            .title("Interview3S 入会")
            .inner_size(1100.0, 820.0)
            .min_inner_size(800.0, 600.0)
            .on_navigation(|url| {
                // Dual-window mode: allow native Tencent Meeting protocol launches.
                let _ = url;
                true
            })
            .build()?;

            let _ = main;

            let _guard_win = WebviewWindowBuilder::new(
                app,
                "guard",
                WebviewUrl::App("guard.html".into()),
            )
            .title("监考守卫")
            .inner_size(380.0, 520.0)
            .resizable(true)
            .always_on_top(true)
            .build()?;

            #[cfg(windows)]
            {
                guard::start_guard_loop(app.handle().clone(), guard_state.clone());
                // Start armed by default for MVP demo.
                guard_state.set_enabled(true);
                guard_state.set_pull_back(true);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
