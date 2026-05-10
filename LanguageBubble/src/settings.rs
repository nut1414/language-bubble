use windows::core::{w, PCWSTR};
use windows::ApplicationModel::{Package, StartupTask, StartupTaskState};
use windows::Win32::System::Registry::*;

use crate::types::*;

const SUBKEY: PCWSTR = w!("Software\\LanguageBubble");
const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP_NAME: PCWSTR = w!("LanguageBubble");
const STARTUP_TASK_ID: &str = "LanguageBubbleStartup";

pub fn is_msix_packaged() -> bool {
    Package::Current().is_ok()
}

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

pub fn get_key_display_mode(key_name: &str, default: DisplayMode) -> DisplayMode {
    let wide: Vec<u16> = key_name.encode_utf16().chain(std::iter::once(0)).collect();
    read_string(PCWSTR(wide.as_ptr()))
        .map(|s| DisplayMode::from_str(&s))
        .unwrap_or(default)
}

pub fn save_key_display_mode(key_name: &str, mode: DisplayMode) {
    let wide: Vec<u16> = key_name.encode_utf16().chain(std::iter::once(0)).collect();
    write_string(PCWSTR(wide.as_ptr()), mode.as_str());
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

pub fn get_theme_mode() -> ThemeMode {
    read_string(w!("ThemeMode"))
        .map(|s| ThemeMode::from_str(&s))
        .unwrap_or(ThemeMode::System)
}

pub fn save_theme_mode(mode: ThemeMode) {
    write_string(w!("ThemeMode"), mode.as_str());
}

pub fn get_custom_theme_colors() -> CustomThemeColors {
    let mut colors = CustomThemeColors::default();
    if let Some(s) = read_string(w!("CustomBG"))
        && let Ok(v) = u32::from_str_radix(&s, 16)
    {
        colors.bg_color = v;
    }
    if let Some(s) = read_string(w!("CustomFG"))
        && let Ok(v) = u32::from_str_radix(&s, 16)
    {
        colors.fg_color = v;
    }
    if let Some(s) = read_string(w!("CustomOpacity"))
        && let Ok(v) = s.parse::<u8>()
    {
        colors.opacity = v;
    }
    colors
}

pub fn save_custom_theme_colors(colors: &CustomThemeColors) {
    write_string(w!("CustomBG"), &format!("{:06X}", colors.bg_color & 0x00FFFFFF));
    write_string(w!("CustomFG"), &format!("{:06X}", colors.fg_color & 0x00FFFFFF));
    write_string(w!("CustomOpacity"), &colors.opacity.to_string());
}

pub fn is_start_with_windows() -> bool {
    if is_msix_packaged() {
        is_start_with_windows_msix()
    } else {
        is_start_with_windows_registry()
    }
}

pub fn set_start_with_windows(enable: bool) {
    if is_msix_packaged() {
        set_start_with_windows_msix(enable);
    } else {
        set_start_with_windows_registry(enable);
    }
}

fn is_start_with_windows_msix() -> bool {
    let Ok(task) = StartupTask::GetAsync(&STARTUP_TASK_ID.into()).and_then(|op| op.get()) else {
        return false;
    };
    let Ok(state) = task.State() else {
        return false;
    };
    matches!(
        state,
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy
    )
}

fn set_start_with_windows_msix(enable: bool) {
    let Ok(task) = StartupTask::GetAsync(&STARTUP_TASK_ID.into()).and_then(|op| op.get()) else {
        return;
    };
    if enable {
        let _ = task.RequestEnableAsync().and_then(|op| op.get());
    } else {
        let _ = task.Disable();
    }
}

fn is_start_with_windows_registry() -> bool {
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

fn set_start_with_windows_registry(enable: bool) {
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

pub fn get_check_for_updates() -> bool {
    read_string(w!("CheckForUpdates")).as_deref() != Some("False")
}

pub fn save_check_for_updates(enabled: bool) {
    write_string(w!("CheckForUpdates"), if enabled { "True" } else { "False" });
}

pub fn get_last_update_check() -> u64 {
    read_string(w!("LastUpdateCheck"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

pub fn save_last_update_check(timestamp: u64) {
    write_string(w!("LastUpdateCheck"), &timestamp.to_string());
}

pub fn get_last_seen_version() -> String {
    read_string(w!("LastSeenVersion")).unwrap_or_default()
}

pub fn save_last_seen_version(version: &str) {
    write_string(w!("LastSeenVersion"), version);
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

pub fn migrate_display_mode_settings() {
    // Check if already migrated
    if read_string(w!("CapsLockDisplayMode")).is_some() {
        return;
    }
    // Copy old global DisplayMode to all three per-key settings
    let mode = read_string(w!("DisplayMode"))
        .map(|s| DisplayMode::from_str(&s))
        .unwrap_or(DisplayMode::Carousel);
    write_string(w!("CapsLockDisplayMode"), mode.as_str());
    write_string(w!("WinSpaceDisplayMode"), mode.as_str());
    write_string(w!("AltShiftDisplayMode"), mode.as_str());
}
