use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::*;

use crate::types::*;

const SUBKEY: PCWSTR = w!("Software\\LanguageBubble");
const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP_NAME: PCWSTR = w!("LanguageBubble");

fn read_string(key_name: PCWSTR) -> Option<String> {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, SUBKEY, Some(0), KEY_READ, &mut hkey).is_err() {
            return None;
        }
        let mut buf = [0u16; 256];
        let mut size = (buf.len() * 2) as u32;
        let mut kind = REG_VALUE_TYPE::default();
        let result = RegQueryValueExW(
            hkey,
            key_name,
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        if result.is_ok() && kind == REG_SZ {
            let len = (size as usize / 2).saturating_sub(1);
            Some(String::from_utf16_lossy(&buf[..len]))
        } else {
            None
        }
    }
}

fn write_string(key_name: PCWSTR, value: &str) {
    unsafe {
        let mut hkey = HKEY::default();
        if RegCreateKeyW(
            HKEY_CURRENT_USER,
            SUBKEY,
            &mut hkey,
        )
        .is_err()
        {
            return;
        }
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = RegSetValueExW(
            hkey,
            key_name,
            Some(0),
            REG_SZ,
            Some(std::slice::from_raw_parts(
                wide.as_ptr() as *const u8,
                wide.len() * 2,
            )),
        );
        let _ = RegCloseKey(hkey);
    }
}

pub fn get_key_mode(key_name: &str, default: SwitchMode) -> SwitchMode {
    let wide: Vec<u16> = key_name.encode_utf16().chain(std::iter::once(0)).collect();
    read_string(PCWSTR(wide.as_ptr()))
        .map(|s| SwitchMode::from_str(&s))
        .unwrap_or(default)
}

pub fn save_key_mode(key_name: &str, mode: SwitchMode) {
    let wide: Vec<u16> = key_name.encode_utf16().chain(std::iter::once(0)).collect();
    write_string(PCWSTR(wide.as_ptr()), mode.as_str());
}

pub fn get_bubble_size() -> BubbleSize {
    read_string(w!("Size"))
        .map(|s| BubbleSize::from_str(&s))
        .unwrap_or(BubbleSize::Medium)
}

pub fn save_bubble_size(size: BubbleSize) {
    write_string(w!("Size"), size.as_str());
}

pub fn get_display_mode() -> DisplayMode {
    read_string(w!("DisplayMode"))
        .map(|s| DisplayMode::from_str(&s))
        .unwrap_or(DisplayMode::Carousel)
}

pub fn save_display_mode(mode: DisplayMode) {
    write_string(w!("DisplayMode"), mode.as_str());
}

pub fn get_hide_on_typing() -> bool {
    read_string(w!("HideOnTyping")).as_deref() == Some("True")
}

pub fn save_hide_on_typing(enabled: bool) {
    write_string(w!("HideOnTyping"), if enabled { "True" } else { "False" });
}

pub fn get_expanded_mru_only() -> bool {
    read_string(w!("ExpandedMruOnly")).as_deref() == Some("True")
}

pub fn save_expanded_mru_only(enabled: bool) {
    write_string(
        w!("ExpandedMruOnly"),
        if enabled { "True" } else { "False" },
    );
}

pub fn is_start_with_windows() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, Some(0), KEY_READ, &mut hkey).is_err() {
            return false;
        }
        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let mut kind = REG_VALUE_TYPE::default();
        let result = RegQueryValueExW(
            hkey,
            APP_NAME,
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        result.is_ok()
    }
}

pub fn set_start_with_windows(enable: bool) {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, Some(0), KEY_WRITE, &mut hkey).is_err() {
            return;
        }
        if enable {
            let exe = std::env::current_exe().unwrap_or_default();
            let path = format!("\"{}\"", exe.display());
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = RegSetValueExW(
                hkey,
                APP_NAME,
                Some(0),
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    wide.as_ptr() as *const u8,
                    wide.len() * 2,
                )),
            );
        } else {
            let _ = RegDeleteValueW(hkey, APP_NAME);
        }
        let _ = RegCloseKey(hkey);
    }
}

pub fn migrate_old_settings() {
    // Check if already migrated
    if read_string(w!("CapsLockMode")).is_some() {
        return;
    }
    let mru = read_string(w!("MruSwitching"));
    let mode = if mru.as_deref() == Some("True") {
        SwitchMode::Mru
    } else {
        SwitchMode::AllLanguage
    };
    write_string(w!("CapsLockMode"), mode.as_str());
    // Delete old key
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, SUBKEY, Some(0), KEY_WRITE, &mut hkey).is_ok() {
            let _ = RegDeleteValueW(hkey, w!("MruSwitching"));
            let _ = RegCloseKey(hkey);
        }
    }
}
