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

/// Whether this navigation should be handed to the OS (browser / protocol handler).
pub fn should_launch_externally(url: &Url) -> bool {
    is_meeting_protocol(url) || is_tencent_meeting_https(url)
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

    // Per MSDN, values > 32 indicate success.
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
