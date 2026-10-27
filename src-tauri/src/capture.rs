use parking_lot::Mutex;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

const DEFAULT_INTERVAL_SECS: u64 = 30;
/// Keep newest N PNGs; prune older files in the shots folder.
const MAX_SHOTS: usize = 200;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub enabled: bool,
    pub interval_secs: u64,
    pub count: u32,
    pub last_path: String,
    pub last_error: String,
    pub save_dir: String,
}

#[derive(Default)]
struct CaptureInner {
    enabled: bool,
    interval_secs: u64,
    count: u32,
    last_path: String,
    last_error: String,
    save_dir: PathBuf,
}

#[derive(Clone, Default)]
pub struct CaptureState {
    inner: Arc<Mutex<CaptureInner>>,
}

impl CaptureState {
    pub fn init_dir(&self, dir: PathBuf) {
        let _ = fs::create_dir_all(&dir);
        let mut g = self.inner.lock();
        g.save_dir = dir;
        if g.interval_secs == 0 {
            g.interval_secs = DEFAULT_INTERVAL_SECS;
        }
    }

    pub fn status(&self) -> CaptureStatus {
        let g = self.inner.lock();
        CaptureStatus {
            enabled: g.enabled,
            interval_secs: if g.interval_secs == 0 {
                DEFAULT_INTERVAL_SECS
            } else {
                g.interval_secs
            },
            count: g.count,
            last_path: g.last_path.clone(),
            last_error: g.last_error.clone(),
            save_dir: g.save_dir.display().to_string(),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.inner.lock().enabled = enabled;
    }

    pub fn set_interval_secs(&self, secs: u64) {
        self.inner.lock().interval_secs = secs.max(5);
    }
}

fn timestamp_name() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Local-ish readable stamp from unix ms (UTC). Good enough for filenames.
    let secs = (ms / 1000) as i64;
    let millis = (ms % 1000) as u32;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400) as u32;
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    let sec = tod % 60;
    // Civil date from days since Unix epoch (1970-01-01).
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}_{hour:02}{min:02}{sec:02}_{millis:03}")
}

/// Algorithm from Howard Hinnant (civil_from_days).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn prune_old_shots(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = rd
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta.modified().ok()?;
            Some((e.path(), modified))
        })
        .collect();
    if files.len() <= MAX_SHOTS {
        return;
    }
    files.sort_by_key(|(_, m)| *m);
    let remove_n = files.len() - MAX_SHOTS;
    for (path, _) in files.into_iter().take(remove_n) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    pub fn grab_virtual_screen_rgba() -> Result<(u32, u32, Vec<u8>), String> {
        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            if w <= 0 || h <= 0 {
                return Err("无法获取屏幕尺寸".into());
            }
            let width = w as u32;
            let height = h as u32;

            let hdc_screen = GetDC(0 as HWND);
            if hdc_screen.is_null() {
                return Err("GetDC 失败".into());
            }
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            if hdc_mem.is_null() {
                ReleaseDC(0 as HWND, hdc_screen);
                return Err("CreateCompatibleDC 失败".into());
            }
            let hbmp = CreateCompatibleBitmap(hdc_screen, w, h);
            if hbmp.is_null() {
                DeleteDC(hdc_mem);
                ReleaseDC(0 as HWND, hdc_screen);
                return Err("CreateCompatibleBitmap 失败".into());
            }
            let old = SelectObject(hdc_mem, hbmp as HGDIOBJ);
            let ok = BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, x, y, SRCCOPY);
            if ok == 0 {
                SelectObject(hdc_mem, old);
                DeleteObject(hbmp as HGDIOBJ);
                DeleteDC(hdc_mem);
                ReleaseDC(0 as HWND, hdc_screen);
                return Err("BitBlt 截屏失败".into());
            }

            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB as u32,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [std::mem::zeroed()],
            };

            let stride = (width * 4) as usize;
            let mut bgra = vec![0u8; stride * height as usize];
            let lines = GetDIBits(
                hdc_mem,
                hbmp,
                0,
                height,
                bgra.as_mut_ptr() as *mut _,
                &mut bmi,
                DIB_RGB_COLORS,
            );

            SelectObject(hdc_mem, old);
            DeleteObject(hbmp as HGDIOBJ);
            DeleteDC(hdc_mem);
            ReleaseDC(0 as HWND, hdc_screen);

            if lines == 0 {
                return Err("GetDIBits 失败".into());
            }

            // BGRA → RGBA for the `image` crate.
            let mut rgba = bgra;
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            Ok((width, height, rgba))
        }
    }
}

#[cfg(not(windows))]
mod win {
    pub fn grab_virtual_screen_rgba() -> Result<(u32, u32, Vec<u8>), String> {
        Err("截屏仅支持 Windows".into())
    }
}

pub fn capture_once(state: &CaptureState) -> Result<PathBuf, String> {
    let dir = {
        let g = state.inner.lock();
        if g.save_dir.as_os_str().is_empty() {
            return Err("截屏目录未初始化".into());
        }
        g.save_dir.clone()
    };
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;

    let (width, height, rgba) = win::grab_virtual_screen_rgba()?;
    let path = dir.join(format!("shot_{}.png", timestamp_name()));

    let img = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "像素缓冲尺寸不匹配".to_string())?;
    img.save(&path)
        .map_err(|e| format!("写入 PNG 失败: {e}"))?;

    prune_old_shots(&dir);

    {
        let mut g = state.inner.lock();
        g.count = g.count.saturating_add(1);
        g.last_path = path.display().to_string();
        g.last_error.clear();
    }
    Ok(path)
}

pub fn start_capture_loop(app: AppHandle, state: CaptureState) {
    thread::spawn(move || loop {
        let (enabled, interval) = {
            let g = state.inner.lock();
            (
                g.enabled,
                if g.interval_secs == 0 {
                    DEFAULT_INTERVAL_SECS
                } else {
                    g.interval_secs
                },
            )
        };
        if enabled {
            match capture_once(&state) {
                Ok(path) => {
                    eprintln!("[capture] saved {}", path.display());
                    let _ = app.emit("capture://saved", state.status());
                }
                Err(e) => {
                    eprintln!("[capture] {e}");
                    state.inner.lock().last_error = e.clone();
                    let _ = app.emit("capture://error", state.status());
                }
            }
        }
        thread::sleep(Duration::from_secs(interval.max(5)));
    });
}

pub fn open_save_dir(state: &CaptureState) -> Result<String, String> {
    let dir = {
        let g = state.inner.lock();
        g.save_dir.clone()
    };
    if dir.as_os_str().is_empty() {
        return Err("截屏目录未初始化".into());
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.display().to_string();
    crate::launch::open_externally(&path)?;
    Ok(path)
}
