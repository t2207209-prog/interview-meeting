use url::Url;

pub fn is_meeting_protocol(url: &Url) -> bool {
    matches!(url.scheme(), "wemeet" | "wemeetios" | "tencentmeeting")
}

pub fn is_tencent_meeting_https(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    url.host_str().is_some_and(|host| {
        let h = host.to_ascii_lowercase();
        h == "meeting.tencent.com"
            || h.ends_with(".meeting.tencent.com")
            || h == "voovmeeting.com"
            || h.ends_with(".voovmeeting.com")
    })
}

pub fn should_launch_externally(url: &Url) -> bool {
    is_meeting_protocol(url) || is_tencent_meeting_https(url)
}

fn digits_only(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Pull a meeting code from query / fragment / path when possible.
pub fn extract_meeting_code(url: &Url) -> Option<String> {
    const KEYS: &[&str] = &[
        "meeting_code",
        "meetingCode",
        "meetingcode",
        "code",
        "mn",
    ];

    for (k, v) in url.query_pairs() {
        if KEYS.iter().any(|key| k.eq_ignore_ascii_case(key)) {
            let digits = digits_only(&v);
            if digits.len() >= 9 {
                return Some(digits);
            }
        }
    }

    // Rare: code in fragment meeting_code=...
    if let Some(frag) = url.fragment() {
        for part in frag.split(&['&', '?', '#'][..]) {
            if let Some((k, v)) = part.split_once('=') {
                if KEYS.iter().any(|key| k.eq_ignore_ascii_case(key)) {
                    let digits = digits_only(v);
                    if digits.len() >= 9 {
                        return Some(digits);
                    }
                }
            }
        }
    }

    None
}

/// Build the native client deep link. This is what triggers the OS/app launch
/// (similar to the browser prompt “要打开腾讯会议吗?”).
pub fn wemeet_deep_link(meeting_code: &str) -> String {
    let code = digits_only(meeting_code);
    format!("wemeet://page/inmeeting?meeting_code={code}")
}

/// Prefer native protocol; fall back to original https only when no code is known.
pub fn resolve_launch_target(url: &Url) -> String {
    if is_meeting_protocol(url) {
        return url.as_str().to_string();
    }
    if let Some(code) = extract_meeting_code(url) {
        return wemeet_deep_link(&code);
    }
    // Short invite links without an embedded code: still open https so the
    // page can continue; caller may also pass a code via hint.
    url.as_str().to_string()
}

pub fn resolve_launch_target_with_hint(url: &Url, code_hint: Option<&str>) -> String {
    if is_meeting_protocol(url) {
        return url.as_str().to_string();
    }
    if let Some(code) = extract_meeting_code(url) {
        return wemeet_deep_link(&code);
    }
    if let Some(hint) = code_hint {
        let digits = digits_only(hint);
        if digits.len() >= 9 {
            return wemeet_deep_link(&digits);
        }
    }
    url.as_str().to_string()
}

#[cfg(windows)]
pub fn open_externally(url: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::core::w;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide: Vec<u16> = std::ffi::OsStr::new(url)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let ret = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            w!("open"),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    if (ret as isize) <= 32 {
        Err(format!("无法打开链接（ShellExecute={ret:?}）：{url}"))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn open_externally(url: &str) -> Result<(), String> {
    Err(format!("当前平台暂不支持外部打开：{url}"))
}

/// Launch native meeting client when possible.
pub fn launch_meeting(url: &Url, code_hint: Option<&str>) -> Result<String, String> {
    let target = resolve_launch_target_with_hint(url, code_hint);
    open_externally(&target)?;
    Ok(target)
}
