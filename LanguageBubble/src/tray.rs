mod command;
mod menu;
pub use command::TrayCommand;
pub use menu::show_context_menu;

use std::mem;

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

const TRAY_ICON_ID: u32 = 1;
pub const WM_TRAY_CALLBACK: u32 = WM_USER + 1;

use crate::types::{BubbleSize, CustomThemeColors, KeyBindings, ThemeMode};

pub struct TrayIcon {
    hwnd: HWND,
    h_icon: HICON,
}

impl TrayIcon {
    pub fn create(hwnd: HWND) -> Self {
        let h_icon = load_embedded_icon().unwrap_or_default();

        let mut nid = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ICON_ID,
            uFlags: NIF_MESSAGE | NIF_TIP | NIF_ICON,
            uCallbackMessage: WM_TRAY_CALLBACK,
            hIcon: h_icon,
            ..Default::default()
        };

        let tip = "Language Bubble";
        let tip_wide: Vec<u16> = tip.encode_utf16().collect();
        let len = tip_wide.len().min(nid.szTip.len() - 1);
        nid.szTip[..len].copy_from_slice(&tip_wide[..len]);

        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }

        TrayIcon { hwnd, h_icon }
    }

    pub fn show_balloon(&self, title: &str, message: &str) {
        let mut nid = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ICON_ID,
            uFlags: NIF_INFO,
            ..Default::default()
        };
        nid.dwInfoFlags = NIIF_INFO;

        let msg_wide: Vec<u16> = message.encode_utf16().collect();
        let msg_len = msg_wide.len().min(nid.szInfo.len() - 1);
        nid.szInfo[..msg_len].copy_from_slice(&msg_wide[..msg_len]);

        let title_wide: Vec<u16> = title.encode_utf16().collect();
        let title_len = title_wide.len().min(nid.szInfoTitle.len() - 1);
        nid.szInfoTitle[..title_len].copy_from_slice(&title_wide[..title_len]);

        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    pub fn remove(&self) {
        let nid = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ICON_ID,
            ..Default::default()
        };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            if !self.h_icon.is_invalid() {
                let _ = DestroyIcon(self.h_icon);
            }
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
    }
}

fn load_embedded_icon() -> Option<HICON> {
    unsafe {
        let hinstance = GetModuleHandleW(None).ok()?;
        let h = LoadImageW(
            Some(hinstance.into()),
            PCWSTR(std::ptr::without_provenance::<u16>(1)), // resource ID 1 (winres default)
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE,
        )
        .ok()?;
        Some(HICON(h.0))
    }
}

pub struct TrayMenuSnapshot {
    pub layouts: Vec<crate::language::LayoutInfo>,
    pub current_hkl: Option<windows::Win32::UI::Input::KeyboardAndMouse::HKL>,
    pub start_with_windows: bool,
    pub size: BubbleSize,
    pub bindings: KeyBindings,
    pub hide_on_typing: bool,
    pub expanded_mru_only: bool,
    pub theme_mode: ThemeMode,
    pub custom_colors: CustomThemeColors,
    pub check_for_updates: bool,
    pub pending_update: Option<String>,
    pub app_version: &'static str,
    pub is_msix: bool,
}
