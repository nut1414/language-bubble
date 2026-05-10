use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const TRAY_ICON_ID: u32 = 1;
pub const WM_TRAY_CALLBACK: u32 = WM_USER + 1;

// Context menu command IDs
pub const CMD_EXIT: u16 = 1000;
pub const CMD_START_WITH_WINDOWS: u16 = 1001;
pub const CMD_HIDE_ON_TYPING: u16 = 1002;
pub const CMD_EXPANDED_MRU_ONLY: u16 = 1003;
pub const CMD_FEEDBACK: u16 = 1004;

// Size: 1100-1104
pub const CMD_SIZE_BASE: u16 = 1100;
// Key bindings switch mode: CapsLock 1300-1302, WinSpace 1310-1312, AltShift 1320-1322
pub const CMD_KEY_CAPSLOCK_BASE: u16 = 1300;
pub const CMD_KEY_WINSPACE_BASE: u16 = 1310;
pub const CMD_KEY_ALTSHIFT_BASE: u16 = 1320;
// Key bindings display mode: CapsLock 1330-1332, WinSpace 1340-1342, AltShift 1350-1352
pub const CMD_KEY_CAPSLOCK_DISPLAY_BASE: u16 = 1330;
pub const CMD_KEY_WINSPACE_DISPLAY_BASE: u16 = 1340;
pub const CMD_KEY_ALTSHIFT_DISPLAY_BASE: u16 = 1350;
// Theme: 1400=System, 1401=Light, 1402=Dark, 1403=Custom
pub const CMD_THEME_BASE: u16 = 1400;
pub const CMD_CUSTOM_BG_COLOR: u16 = 1410;
pub const CMD_CUSTOM_FG_COLOR: u16 = 1411;
pub const CMD_OPACITY_BASE: u16 = 1420;
pub const CMD_CHECK_UPDATES_TOGGLE: u16 = 1503;
pub const CMD_DOWNLOAD_UPDATE: u16 = 1504;

pub struct TrayIcon {
    hwnd: HWND,
    h_icon: HICON,
}

impl TrayIcon {
    pub fn create(hwnd: HWND) -> Self {
        let h_icon = load_embedded_icon().unwrap_or(HICON::default());

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
            PCWSTR(1 as *const u16), // resource ID 1 (winres default)
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE,
        )
        .ok()?;
        Some(HICON(h.0))
    }
}

pub fn show_context_menu(
    hwnd: HWND,
    layouts: &[crate::language::LayoutInfo],
    current_hkl: Option<windows::Win32::UI::Input::KeyboardAndMouse::HKL>,
    start_with_windows: bool,
    size: crate::types::BubbleSize,
    caps_lock_mode: crate::types::SwitchMode,
    win_space_mode: crate::types::SwitchMode,
    alt_shift_mode: crate::types::SwitchMode,
    caps_lock_display: crate::types::DisplayMode,
    win_space_display: crate::types::DisplayMode,
    alt_shift_display: crate::types::DisplayMode,
    hide_on_typing: bool,
    expanded_mru_only: bool,
    theme_mode: crate::types::ThemeMode,
    custom_colors: &crate::types::CustomThemeColors,
    check_for_updates: bool,
    pending_update: Option<&str>,
    app_version: &str,
    is_msix: bool,
) -> Option<u16> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;

        // Header
        let _ = AppendMenuW(menu, MF_STRING | MF_DISABLED, 0, w!("Languages"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        // Language items
        for layout in layouts {
            let text = format!("{} - {}", layout.bubble_text, layout.english_name);
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let mut flags = MF_STRING;
            if current_hkl.is_some_and(|h| h == layout.hkl) {
                flags |= MF_CHECKED;
            }
            flags |= MF_DISABLED;
            let _ = AppendMenuW(menu, flags, 0, PCWSTR(wide.as_ptr()));
        }

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        // Start with Windows
        let sww_flags = MF_STRING | if start_with_windows { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, sww_flags, CMD_START_WITH_WINDOWS as usize, w!("Start with Windows"));

        // Size submenu
        let size_menu = CreatePopupMenu().ok()?;
        let sizes = [
            ("Extra Small", crate::types::BubbleSize::ExtraSmall),
            ("Small", crate::types::BubbleSize::Small),
            ("Medium", crate::types::BubbleSize::Medium),
            ("Large", crate::types::BubbleSize::Large),
            ("Extra Large", crate::types::BubbleSize::ExtraLarge),
        ];
        for (i, (label, s)) in sizes.iter().enumerate() {
            let flags = MF_STRING | if *s == size { MF_CHECKED } else { MF_UNCHECKED };
            let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(size_menu, flags, (CMD_SIZE_BASE + i as u16) as usize, PCWSTR(wide.as_ptr()));
        }
        let _ = AppendMenuW(menu, MF_POPUP, size_menu.0 as usize, w!("Size"));

        let theme_menu = CreatePopupMenu().ok()?;
        let themes = [
            ("System (Auto)", crate::types::ThemeMode::System),
            ("Light", crate::types::ThemeMode::Light),
            ("Dark", crate::types::ThemeMode::Dark),
            ("Custom", crate::types::ThemeMode::Custom),
        ];
        for (i, (label, t)) in themes.iter().enumerate() {
            let flags = MF_STRING | if *t == theme_mode { MF_CHECKED } else { MF_UNCHECKED };
            let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(theme_menu, flags, (CMD_THEME_BASE + i as u16) as usize, PCWSTR(wide.as_ptr()));
        }

        let _ = AppendMenuW(theme_menu, MF_SEPARATOR, 0, None);

        let customize_menu = CreatePopupMenu().ok()?;

        let bg_label = "Background Color...";
        let bg_wide: Vec<u16> = bg_label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(customize_menu, MF_STRING, CMD_CUSTOM_BG_COLOR as usize, PCWSTR(bg_wide.as_ptr()));

        let fg_label = "Text Color...";
        let fg_wide: Vec<u16> = fg_label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(customize_menu, MF_STRING, CMD_CUSTOM_FG_COLOR as usize, PCWSTR(fg_wide.as_ptr()));

        let _ = AppendMenuW(customize_menu, MF_SEPARATOR, 0, None);

        let opacity_menu = CreatePopupMenu().ok()?;
        let opacity_labels = ["25%", "50%", "75%", "85%", "90%", "95%", "100%"];
        let opacity_values = crate::types::OPACITY_VALUES;
        for (i, label) in opacity_labels.iter().enumerate() {
            let flags = MF_STRING | if opacity_values[i] == custom_colors.opacity { MF_CHECKED } else { MF_UNCHECKED };
            let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(opacity_menu, flags, (CMD_OPACITY_BASE + i as u16) as usize, PCWSTR(wide.as_ptr()));
        }
        let opacity_wide: Vec<u16> = "Opacity".encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(customize_menu, MF_POPUP, opacity_menu.0 as usize, PCWSTR(opacity_wide.as_ptr()));

        let customize_flags = if theme_mode == crate::types::ThemeMode::Custom {
            MF_POPUP
        } else {
            MF_POPUP | MF_GRAYED
        };
        let customize_label: Vec<u16> = "Customize...".encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(theme_menu, customize_flags, customize_menu.0 as usize, PCWSTR(customize_label.as_ptr()));

        let _ = AppendMenuW(menu, MF_POPUP, theme_menu.0 as usize, w!("Theme"));

        // Key Bindings submenu (now includes display mode per key)
        let key_menu = CreatePopupMenu().ok()?;
        add_key_submenu(key_menu, "CapsLock", CMD_KEY_CAPSLOCK_BASE, caps_lock_mode, CMD_KEY_CAPSLOCK_DISPLAY_BASE, caps_lock_display);
        add_key_submenu(key_menu, "Win + Space", CMD_KEY_WINSPACE_BASE, win_space_mode, CMD_KEY_WINSPACE_DISPLAY_BASE, win_space_display);
        add_key_submenu(key_menu, "Alt + Shift", CMD_KEY_ALTSHIFT_BASE, alt_shift_mode, CMD_KEY_ALTSHIFT_DISPLAY_BASE, alt_shift_display);
        let _ = AppendMenuW(menu, MF_POPUP, key_menu.0 as usize, w!("Key Bindings"));

        // Hide on typing
        let hot_flags = MF_STRING | if hide_on_typing { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, hot_flags, CMD_HIDE_ON_TYPING as usize, w!("Hide on Typing"));

        // Show Only Recent Languages (for Expanded mode)
        let mru_flags = MF_STRING | if expanded_mru_only { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, mru_flags, CMD_EXPANDED_MRU_ONLY as usize, w!("Show Only Recent Languages"));

        // Advanced submenu
        let advanced_menu = CreatePopupMenu().ok()?;
        let version_label = format!("Version {}", app_version);
        let version_wide: Vec<u16> = version_label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(advanced_menu, MF_STRING | MF_DISABLED | MF_GRAYED, 0, PCWSTR(version_wide.as_ptr()));

        if !is_msix {
            let _ = AppendMenuW(advanced_menu, MF_SEPARATOR, 0, None);
            let check_label = "Check for updates";
            let check_wide: Vec<u16> = check_label.encode_utf16().chain(std::iter::once(0)).collect();
            let check_flags = MF_STRING | if check_for_updates { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(advanced_menu, check_flags, CMD_CHECK_UPDATES_TOGGLE as usize, PCWSTR(check_wide.as_ptr()));
        }

        if pending_update.is_some() {
            let dl_label = "Download update...";
            let dl_wide: Vec<u16> = dl_label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(advanced_menu, MF_STRING, CMD_DOWNLOAD_UPDATE as usize, PCWSTR(dl_wide.as_ptr()));
        }

        let _ = AppendMenuW(menu, MF_POPUP, advanced_menu.0 as usize, w!("Advanced"));

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        // Feedback
        let _ = AppendMenuW(menu, MF_STRING, CMD_FEEDBACK as usize, w!("Feedback"));

        // Exit
        let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, w!("Exit"));

        // Show the menu
        let _ = SetForegroundWindow(hwnd);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let cmd = TrackPopupMenuEx(
            menu,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
            cursor.x,
            cursor.y,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);

        if cmd.0 != 0 {
            Some(cmd.0 as u16)
        } else {
            None
        }
    }
}

unsafe fn add_key_submenu(
    parent: HMENU,
    label: &str,
    switch_base_cmd: u16,
    current_switch: crate::types::SwitchMode,
    display_base_cmd: u16,
    current_display: crate::types::DisplayMode,
) {
    unsafe {
        let sub = CreatePopupMenu().unwrap();
        let switch_labels = [
            ("Cycle All Languages", crate::types::SwitchMode::AllLanguage),
            ("Recent and English", crate::types::SwitchMode::Mru),
            ("Do Not Intercept", crate::types::SwitchMode::Unused),
        ];
        for (i, (ml, mv)) in switch_labels.iter().enumerate() {
            let flags = MF_STRING | if *mv == current_switch { MF_CHECKED } else { MF_UNCHECKED };
            let wide: Vec<u16> = ml.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(sub, flags, (switch_base_cmd + i as u16) as usize, PCWSTR(wide.as_ptr()));
        }

        let _ = AppendMenuW(sub, MF_SEPARATOR, 0, None);

        let display_labels = [
            ("Carousel", crate::types::DisplayMode::Carousel),
            ("Simple", crate::types::DisplayMode::Simple),
            ("Show All Languages", crate::types::DisplayMode::Expanded),
        ];
        for (i, (ml, mv)) in display_labels.iter().enumerate() {
            let flags = MF_STRING | if *mv == current_display { MF_CHECKED } else { MF_UNCHECKED };
            let wide: Vec<u16> = ml.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(sub, flags, (display_base_cmd + i as u16) as usize, PCWSTR(wide.as_ptr()));
        }

        let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(parent, MF_POPUP, sub.0 as usize, PCWSTR(wide.as_ptr()));
    }
}
