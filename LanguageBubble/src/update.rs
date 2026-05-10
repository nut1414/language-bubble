use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::Networking::WinHttp::*;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_USER};

const USER_AGENT: &str = concat!("language-bubble/", env!("CARGO_PKG_VERSION"));

pub const WM_UPDATE_AVAILABLE: u32 = WM_USER + 2;
pub static PENDING_UPDATE: Mutex<Option<String>> = Mutex::new(None);

struct HttpHandle(*mut std::ffi::c_void);

impl Drop for HttpHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

pub fn check_in_background(hwnd: HWND) {
    let hwnd_value = hwnd.0 as isize;
    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_value as *mut _);

        if let Some(tag) = fetch_latest_tag() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            crate::settings::save_last_update_check(now);

            let current = env!("CARGO_PKG_VERSION");
            let old_last_seen = crate::settings::get_last_seen_version();

            if is_newer(&tag, &old_last_seen) {
                crate::settings::save_last_seen_version(&tag);
            }

            if is_newer(&tag, current) && is_newer(&tag, &old_last_seen) {
                if let Ok(mut guard) = PENDING_UPDATE.lock() {
                    *guard = Some(tag);
                    unsafe {
                        let _ = PostMessageW(
                            Some(hwnd),
                            WM_UPDATE_AVAILABLE,
                            windows::Win32::Foundation::WPARAM(0),
                            windows::Win32::Foundation::LPARAM(0),
                        );
                    }
                }
            }
        }
    });
}

fn fetch_latest_tag() -> Option<String> {
    unsafe {
        let ua_wide: Vec<u16> = USER_AGENT.encode_utf16().chain(std::iter::once(0)).collect();
        let session = HttpHandle(WinHttpOpen(
            PCWSTR(ua_wide.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR(std::ptr::null()),
            PCWSTR(std::ptr::null()),
            0,
        ));
        if session.0.is_null() {
            return None;
        }

        let connect = HttpHandle(WinHttpConnect(
            session.0,
            w!("api.github.com"),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ));
        if connect.0.is_null() {
            return None;
        }

        let request = HttpHandle(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            w!("/repos/nut1414/language-bubble/releases/latest"),
            PCWSTR(std::ptr::null()),
            PCWSTR(std::ptr::null()),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return None;
        }

        let _ = WinHttpSetTimeouts(request.0, 5000, 5000, 5000, 10000);

        let headers_str = format!(
            "Accept: application/vnd.github+json\r\nUser-Agent: {}\r\n",
            USER_AGENT
        );
        let headers_wide: Vec<u16> = headers_str.encode_utf16().collect();
        let _ = WinHttpAddRequestHeaders(
            request.0,
            &headers_wide,
            WINHTTP_ADDREQ_FLAG_ADD,
        );

        if WinHttpSendRequest(request.0, None, None, 0, 0, 0).is_err() {
            return None;
        }
        if WinHttpReceiveResponse(request.0, std::ptr::null_mut()).is_err() {
            return None;
        }

        let mut body = Vec::new();
        loop {
            let mut available: u32 = 0;
            if WinHttpQueryDataAvailable(request.0, &mut available).is_err() {
                return None;
            }
            if available == 0 {
                break;
            }
            if body.len() + available as usize > 64 * 1024 {
                return None;
            }
            let mut chunk = vec![0u8; available as usize];
            let mut read: u32 = 0;
            if WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut _,
                available,
                &mut read,
            ).is_err() {
                return None;
            }
            chunk.truncate(read as usize);
            body.extend_from_slice(&chunk);
        }

        let json = String::from_utf8(body).ok()?;
        let tag = extract_tag_name(&json)?;
        if is_valid_tag(&tag) {
            Some(tag)
        } else {
            None
        }
    }
}

fn extract_tag_name(json: &str) -> Option<String> {
    let idx = json.find("\"tag_name\"")?;
    let rest = &json[idx + 10..];
    let mut chars = rest.chars();
    // skip whitespace and colon
    let mut found_colon = false;
    while let Some(c) = chars.next() {
        if c == ':' {
            found_colon = true;
            break;
        }
        if !c.is_whitespace() {
            return None;
        }
    }
    if !found_colon {
        return None;
    }
    // skip whitespace and opening quote
    let mut found_quote = false;
    while let Some(c) = chars.next() {
        if c == '"' {
            found_quote = true;
            break;
        }
        if !c.is_whitespace() {
            return None;
        }
    }
    if !found_quote {
        return None;
    }
    let mut tag = String::new();
    for c in chars {
        if c == '"' {
            return Some(tag);
        }
        tag.push(c);
    }
    None
}

fn is_valid_tag(tag: &str) -> bool {
    if tag.is_empty() || tag.len() > 64 {
        return false;
    }
    tag.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim_start_matches('v');
    // Strict: any pre-release/build suffix means "doesn't parse cleanly" → caller treats as no-update.
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse::<u32>().ok()?;
    let minor = parts[1].parse::<u32>().ok()?;
    let patch = parts[2].parse::<u32>().ok()?;
    Some((major, minor, patch))
}

/// On startup, restore the "Download update..." menu entry if the registry says
/// we previously saw a release newer than what's currently installed.
pub fn pending_from_registry() -> Option<String> {
    let current = env!("CARGO_PKG_VERSION");
    let last_seen = crate::settings::get_last_seen_version();
    if last_seen.is_empty() || !is_valid_tag(&last_seen) {
        return None;
    }
    if is_newer(&last_seen, current) {
        Some(last_seen)
    } else {
        None
    }
}

fn is_newer(latest: &str, current: &str) -> bool {
    if current.is_empty() {
        return true;
    }
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        (Some(_), None) => true,
        _ => false,
    }
}
