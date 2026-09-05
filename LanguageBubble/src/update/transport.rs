use super::policy::{extract_tag_name, is_valid_tag};
use windows::Win32::Networking::WinHttp::*;
use windows::core::{PCWSTR, w};

const USER_AGENT: &str = concat!("language-bubble/", env!("CARGO_PKG_VERSION"));

struct HttpHandle(*mut std::ffi::c_void);

impl Drop for HttpHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

pub(super) fn fetch_latest_tag() -> Option<String> {
    unsafe {
        let ua_wide: Vec<u16> = USER_AGENT
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
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
        let _ = WinHttpAddRequestHeaders(request.0, &headers_wide, WINHTTP_ADDREQ_FLAG_ADD);

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
            )
            .is_err()
            {
                return None;
            }
            chunk.truncate(read as usize);
            body.extend_from_slice(&chunk);
        }

        let json = String::from_utf8(body).ok()?;
        let tag = extract_tag_name(&json)?;
        if is_valid_tag(&tag) { Some(tag) } else { None }
    }
}
