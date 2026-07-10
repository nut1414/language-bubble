use std::mem;

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

const TRAY_ICON_ID: u32 = 1;
pub const WM_TRAY_CALLBACK: u32 = WM_USER + 1;

use crate::types::{
    BubbleSize, CustomThemeColors, DisplayMode, HookKeyCombo, KeyBindingConfig, KeyBindings,
    OPACITY_VALUES, SwitchMode, ThemeMode,
};

// Context menu command IDs
const CMD_EXIT: u16 = 1000;
const CMD_START_WITH_WINDOWS: u16 = 1001;
const CMD_HIDE_ON_TYPING: u16 = 1002;
const CMD_EXPANDED_MRU_ONLY: u16 = 1003;
const CMD_FEEDBACK: u16 = 1004;

// Size: 1100-1104
const CMD_SIZE_BASE: u16 = 1100;
// Key bindings switch mode: CapsLock 1300-1302, WinSpace 1310-1312, AltShift 1320-1322
const CMD_KEY_CAPSLOCK_BASE: u16 = 1300;
const CMD_KEY_WINSPACE_BASE: u16 = 1310;
const CMD_KEY_ALTSHIFT_BASE: u16 = 1320;
// Key bindings display mode: CapsLock 1330-1332, WinSpace 1340-1342, AltShift 1350-1352
const CMD_KEY_CAPSLOCK_DISPLAY_BASE: u16 = 1330;
const CMD_KEY_WINSPACE_DISPLAY_BASE: u16 = 1340;
const CMD_KEY_ALTSHIFT_DISPLAY_BASE: u16 = 1350;
// Theme: 1400=System, 1401=Light, 1402=Dark, 1403=Custom
const CMD_THEME_BASE: u16 = 1400;
const CMD_CUSTOM_BG_COLOR: u16 = 1410;
const CMD_CUSTOM_FG_COLOR: u16 = 1411;
const CMD_OPACITY_BASE: u16 = 1420;
const CMD_CHECK_UPDATES_TOGGLE: u16 = 1503;
const CMD_DOWNLOAD_UPDATE: u16 = 1504;

const SIZE_VALUES: [BubbleSize; 5] = [
    BubbleSize::ExtraSmall,
    BubbleSize::Small,
    BubbleSize::Medium,
    BubbleSize::Large,
    BubbleSize::ExtraLarge,
];
const SWITCH_MODE_VALUES: [SwitchMode; 3] =
    [SwitchMode::AllLanguage, SwitchMode::Mru, SwitchMode::Unused];
const DISPLAY_MODE_VALUES: [DisplayMode; 3] = [
    DisplayMode::Carousel,
    DisplayMode::Simple,
    DisplayMode::Expanded,
];
const THEME_VALUES: [ThemeMode; 4] = [
    ThemeMode::System,
    ThemeMode::Light,
    ThemeMode::Dark,
    ThemeMode::Custom,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Exit,
    ToggleStartWithWindows,
    ToggleHideOnTyping,
    ToggleExpandedMruOnly,
    Feedback,
    SetSize(BubbleSize),
    SetSwitchMode {
        combo: HookKeyCombo,
        mode: SwitchMode,
    },
    SetDisplayMode {
        combo: HookKeyCombo,
        mode: DisplayMode,
    },
    SetTheme(ThemeMode),
    PickCustomBackground,
    PickCustomForeground,
    SetOpacity(u8),
    ToggleUpdateChecks,
    DownloadUpdate,
}

impl TrayCommand {
    fn id(self) -> Option<u16> {
        let id = match self {
            Self::Exit => CMD_EXIT,
            Self::ToggleStartWithWindows => CMD_START_WITH_WINDOWS,
            Self::ToggleHideOnTyping => CMD_HIDE_ON_TYPING,
            Self::ToggleExpandedMruOnly => CMD_EXPANDED_MRU_ONLY,
            Self::Feedback => CMD_FEEDBACK,
            Self::SetSize(size) => CMD_SIZE_BASE + value_index(&SIZE_VALUES, size)? as u16,
            Self::SetSwitchMode { combo, mode } => {
                switch_mode_base(combo) + value_index(&SWITCH_MODE_VALUES, mode)? as u16
            }
            Self::SetDisplayMode { combo, mode } => {
                display_mode_base(combo) + value_index(&DISPLAY_MODE_VALUES, mode)? as u16
            }
            Self::SetTheme(theme) => CMD_THEME_BASE + value_index(&THEME_VALUES, theme)? as u16,
            Self::PickCustomBackground => CMD_CUSTOM_BG_COLOR,
            Self::PickCustomForeground => CMD_CUSTOM_FG_COLOR,
            Self::SetOpacity(value) => {
                CMD_OPACITY_BASE + value_index(&OPACITY_VALUES, value)? as u16
            }
            Self::ToggleUpdateChecks => CMD_CHECK_UPDATES_TOGGLE,
            Self::DownloadUpdate => CMD_DOWNLOAD_UPDATE,
        };
        Some(id)
    }

    fn from_id(id: u16) -> Option<Self> {
        match id {
            CMD_EXIT => return Some(Self::Exit),
            CMD_START_WITH_WINDOWS => return Some(Self::ToggleStartWithWindows),
            CMD_HIDE_ON_TYPING => return Some(Self::ToggleHideOnTyping),
            CMD_EXPANDED_MRU_ONLY => return Some(Self::ToggleExpandedMruOnly),
            CMD_FEEDBACK => return Some(Self::Feedback),
            CMD_CUSTOM_BG_COLOR => return Some(Self::PickCustomBackground),
            CMD_CUSTOM_FG_COLOR => return Some(Self::PickCustomForeground),
            CMD_CHECK_UPDATES_TOGGLE => return Some(Self::ToggleUpdateChecks),
            CMD_DOWNLOAD_UPDATE => return Some(Self::DownloadUpdate),
            _ => {}
        }

        if let Some(size) = value_from_id(id, CMD_SIZE_BASE, &SIZE_VALUES) {
            return Some(Self::SetSize(size));
        }
        if let Some(theme) = value_from_id(id, CMD_THEME_BASE, &THEME_VALUES) {
            return Some(Self::SetTheme(theme));
        }
        if let Some(value) = value_from_id(id, CMD_OPACITY_BASE, &OPACITY_VALUES) {
            return Some(Self::SetOpacity(value));
        }

        for combo in HookKeyCombo::ALL {
            if let Some(mode) = value_from_id(id, switch_mode_base(combo), &SWITCH_MODE_VALUES) {
                return Some(Self::SetSwitchMode { combo, mode });
            }
            if let Some(mode) = value_from_id(id, display_mode_base(combo), &DISPLAY_MODE_VALUES) {
                return Some(Self::SetDisplayMode { combo, mode });
            }
        }

        None
    }
}

fn value_index<T: Copy + PartialEq>(values: &[T], value: T) -> Option<usize> {
    values.iter().position(|candidate| *candidate == value)
}

fn value_from_id<T: Copy>(id: u16, base: u16, values: &[T]) -> Option<T> {
    let index = id.checked_sub(base)? as usize;
    values.get(index).copied()
}

const fn switch_mode_base(combo: HookKeyCombo) -> u16 {
    match combo {
        HookKeyCombo::CapsLock => CMD_KEY_CAPSLOCK_BASE,
        HookKeyCombo::WinSpace => CMD_KEY_WINSPACE_BASE,
        HookKeyCombo::AltShift => CMD_KEY_ALTSHIFT_BASE,
    }
}

const fn display_mode_base(combo: HookKeyCombo) -> u16 {
    match combo {
        HookKeyCombo::CapsLock => CMD_KEY_CAPSLOCK_DISPLAY_BASE,
        HookKeyCombo::WinSpace => CMD_KEY_WINSPACE_DISPLAY_BASE,
        HookKeyCombo::AltShift => CMD_KEY_ALTSHIFT_DISPLAY_BASE,
    }
}

fn command_id(command: TrayCommand) -> usize {
    command.id().expect("menu command must have a stable ID") as usize
}

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

pub struct ContextMenuParams<'a> {
    pub hwnd: HWND,
    pub layouts: &'a [crate::language::LayoutInfo],
    pub current_hkl: Option<windows::Win32::UI::Input::KeyboardAndMouse::HKL>,
    pub start_with_windows: bool,
    pub size: BubbleSize,
    pub bindings: &'a KeyBindings,
    pub hide_on_typing: bool,
    pub expanded_mru_only: bool,
    pub theme_mode: ThemeMode,
    pub custom_colors: &'a CustomThemeColors,
    pub check_for_updates: bool,
    pub pending_update: Option<&'a str>,
    pub app_version: &'a str,
    pub is_msix: bool,
}

pub fn show_context_menu(p: ContextMenuParams) -> Option<TrayCommand> {
    let ContextMenuParams {
        hwnd,
        layouts,
        current_hkl,
        start_with_windows,
        size,
        bindings,
        hide_on_typing,
        expanded_mru_only,
        theme_mode,
        custom_colors,
        check_for_updates,
        pending_update,
        app_version,
        is_msix,
    } = p;
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
        let sww_flags = MF_STRING
            | if start_with_windows {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
        let _ = AppendMenuW(
            menu,
            sww_flags,
            command_id(TrayCommand::ToggleStartWithWindows),
            w!("Start with Windows"),
        );

        // Size submenu
        let size_menu = CreatePopupMenu().ok()?;
        let sizes = [
            ("Extra Small", BubbleSize::ExtraSmall),
            ("Small", BubbleSize::Small),
            ("Medium", BubbleSize::Medium),
            ("Large", BubbleSize::Large),
            ("Extra Large", BubbleSize::ExtraLarge),
        ];
        for (label, s) in sizes.iter() {
            let flags = MF_STRING | if *s == size { MF_CHECKED } else { MF_UNCHECKED };
            let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(
                size_menu,
                flags,
                command_id(TrayCommand::SetSize(*s)),
                PCWSTR(wide.as_ptr()),
            );
        }
        let _ = AppendMenuW(menu, MF_POPUP, size_menu.0 as usize, w!("Size"));

        let theme_menu = CreatePopupMenu().ok()?;
        let themes = [
            ("System (Auto)", ThemeMode::System),
            ("Light", ThemeMode::Light),
            ("Dark", ThemeMode::Dark),
            ("Custom", ThemeMode::Custom),
        ];
        for (label, t) in themes.iter() {
            let flags = MF_STRING
                | if *t == theme_mode {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                };
            let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(
                theme_menu,
                flags,
                command_id(TrayCommand::SetTheme(*t)),
                PCWSTR(wide.as_ptr()),
            );
        }

        let _ = AppendMenuW(theme_menu, MF_SEPARATOR, 0, None);

        let customize_menu = CreatePopupMenu().ok()?;

        let bg_label = "Background Color...";
        let bg_wide: Vec<u16> = bg_label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(
            customize_menu,
            MF_STRING,
            command_id(TrayCommand::PickCustomBackground),
            PCWSTR(bg_wide.as_ptr()),
        );

        let fg_label = "Text Color...";
        let fg_wide: Vec<u16> = fg_label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(
            customize_menu,
            MF_STRING,
            command_id(TrayCommand::PickCustomForeground),
            PCWSTR(fg_wide.as_ptr()),
        );

        let _ = AppendMenuW(customize_menu, MF_SEPARATOR, 0, None);

        let opacity_menu = CreatePopupMenu().ok()?;
        let opacity_labels = ["25%", "50%", "75%", "85%", "90%", "95%", "100%"];
        let opacity_values = OPACITY_VALUES;
        for (i, label) in opacity_labels.iter().enumerate() {
            let flags = MF_STRING
                | if opacity_values[i] == custom_colors.opacity {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                };
            let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(
                opacity_menu,
                flags,
                command_id(TrayCommand::SetOpacity(opacity_values[i])),
                PCWSTR(wide.as_ptr()),
            );
        }
        let opacity_wide: Vec<u16> = "Opacity".encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(
            customize_menu,
            MF_POPUP,
            opacity_menu.0 as usize,
            PCWSTR(opacity_wide.as_ptr()),
        );

        let customize_flags = if theme_mode == ThemeMode::Custom {
            MF_POPUP
        } else {
            MF_POPUP | MF_GRAYED
        };
        let customize_label: Vec<u16> = "Customize..."
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let _ = AppendMenuW(
            theme_menu,
            customize_flags,
            customize_menu.0 as usize,
            PCWSTR(customize_label.as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_POPUP, theme_menu.0 as usize, w!("Theme"));

        // Key Bindings submenu (now includes display mode per key)
        let key_menu = CreatePopupMenu().ok()?;
        for combo in HookKeyCombo::ALL {
            add_key_submenu(key_menu, combo, bindings.get(combo));
        }
        let _ = AppendMenuW(menu, MF_POPUP, key_menu.0 as usize, w!("Key Bindings"));

        // Hide on typing
        let hot_flags = MF_STRING
            | if hide_on_typing {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
        let _ = AppendMenuW(
            menu,
            hot_flags,
            command_id(TrayCommand::ToggleHideOnTyping),
            w!("Hide on Typing"),
        );

        // Show Only Recent Languages (for Expanded mode)
        let mru_flags = MF_STRING
            | if expanded_mru_only {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
        let _ = AppendMenuW(
            menu,
            mru_flags,
            command_id(TrayCommand::ToggleExpandedMruOnly),
            w!("Show Only Recent Languages"),
        );

        // Advanced submenu
        let advanced_menu = CreatePopupMenu().ok()?;
        let version_label = format!("Version {}", app_version);
        let version_wide: Vec<u16> = version_label
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let _ = AppendMenuW(
            advanced_menu,
            MF_STRING | MF_DISABLED | MF_GRAYED,
            0,
            PCWSTR(version_wide.as_ptr()),
        );

        if !is_msix {
            let _ = AppendMenuW(advanced_menu, MF_SEPARATOR, 0, None);
            let check_label = "Check for updates";
            let check_wide: Vec<u16> = check_label
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let check_flags = MF_STRING
                | if check_for_updates {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                };
            let _ = AppendMenuW(
                advanced_menu,
                check_flags,
                command_id(TrayCommand::ToggleUpdateChecks),
                PCWSTR(check_wide.as_ptr()),
            );
        }

        if !is_msix && let Some(pending) = pending_update {
            let dl_label = format!("Download update... ({})", pending);
            let dl_wide: Vec<u16> = dl_label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(
                advanced_menu,
                MF_STRING,
                command_id(TrayCommand::DownloadUpdate),
                PCWSTR(dl_wide.as_ptr()),
            );
        }

        let _ = AppendMenuW(menu, MF_POPUP, advanced_menu.0 as usize, w!("Advanced"));

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        // Feedback
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            command_id(TrayCommand::Feedback),
            w!("Feedback"),
        );

        // Exit
        let _ = AppendMenuW(menu, MF_STRING, command_id(TrayCommand::Exit), w!("Exit"));

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
            TrayCommand::from_id(cmd.0 as u16)
        } else {
            None
        }
    }
}

unsafe fn add_key_submenu(parent: HMENU, combo: HookKeyCombo, binding: KeyBindingConfig) {
    unsafe {
        let sub = CreatePopupMenu().unwrap();
        let label = match combo {
            HookKeyCombo::CapsLock => "CapsLock",
            HookKeyCombo::WinSpace => "Win + Space",
            HookKeyCombo::AltShift => "Alt + Shift",
        };
        let switch_labels = [
            ("Cycle All Languages", SwitchMode::AllLanguage),
            ("Recent and English", SwitchMode::Mru),
            ("Do Not Intercept", SwitchMode::Unused),
        ];
        for (menu_label, mode) in switch_labels {
            let flags = MF_STRING
                | if mode == binding.switch_mode {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                };
            let wide: Vec<u16> = menu_label
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let _ = AppendMenuW(
                sub,
                flags,
                command_id(TrayCommand::SetSwitchMode { combo, mode }),
                PCWSTR(wide.as_ptr()),
            );
        }

        let _ = AppendMenuW(sub, MF_SEPARATOR, 0, None);

        let display_labels = [
            ("Carousel", DisplayMode::Carousel),
            ("Simple", DisplayMode::Simple),
            ("Show All Languages", DisplayMode::Expanded),
        ];
        for (menu_label, mode) in display_labels {
            let flags = MF_STRING
                | if mode == binding.display_mode {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                };
            let wide: Vec<u16> = menu_label
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let _ = AppendMenuW(
                sub,
                flags,
                command_id(TrayCommand::SetDisplayMode { combo, mode }),
                PCWSTR(wide.as_ptr()),
            );
        }

        let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(parent, MF_POPUP, sub.0 as usize, PCWSTR(wide.as_ptr()));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn all_commands() -> Vec<TrayCommand> {
        let mut commands = vec![
            TrayCommand::Exit,
            TrayCommand::ToggleStartWithWindows,
            TrayCommand::ToggleHideOnTyping,
            TrayCommand::ToggleExpandedMruOnly,
            TrayCommand::Feedback,
            TrayCommand::PickCustomBackground,
            TrayCommand::PickCustomForeground,
            TrayCommand::ToggleUpdateChecks,
            TrayCommand::DownloadUpdate,
        ];
        commands.extend(SIZE_VALUES.into_iter().map(TrayCommand::SetSize));
        commands.extend(THEME_VALUES.into_iter().map(TrayCommand::SetTheme));
        commands.extend(OPACITY_VALUES.into_iter().map(TrayCommand::SetOpacity));
        for combo in HookKeyCombo::ALL {
            commands.extend(
                SWITCH_MODE_VALUES
                    .into_iter()
                    .map(|mode| TrayCommand::SetSwitchMode { combo, mode }),
            );
            commands.extend(
                DISPLAY_MODE_VALUES
                    .into_iter()
                    .map(|mode| TrayCommand::SetDisplayMode { combo, mode }),
            );
        }
        commands
    }

    #[test]
    fn every_menu_command_id_round_trips_and_is_unique() {
        let commands = all_commands();
        let mut ids = HashSet::new();
        for command in commands {
            let id = command.id().expect("supported command should have an ID");
            assert!(ids.insert(id), "duplicate command ID: {id}");
            assert_eq!(TrayCommand::from_id(id), Some(command));
        }
    }

    #[test]
    fn command_ids_preserve_win32_compatibility() {
        let fixed = [
            (TrayCommand::Exit, 1000),
            (TrayCommand::ToggleStartWithWindows, 1001),
            (TrayCommand::ToggleHideOnTyping, 1002),
            (TrayCommand::ToggleExpandedMruOnly, 1003),
            (TrayCommand::Feedback, 1004),
            (TrayCommand::PickCustomBackground, 1410),
            (TrayCommand::PickCustomForeground, 1411),
            (TrayCommand::ToggleUpdateChecks, 1503),
            (TrayCommand::DownloadUpdate, 1504),
        ];
        for (command, expected) in fixed {
            assert_eq!(command.id(), Some(expected));
        }

        for (index, size) in SIZE_VALUES.into_iter().enumerate() {
            assert_eq!(TrayCommand::SetSize(size).id(), Some(1100 + index as u16));
        }
        for (index, theme) in THEME_VALUES.into_iter().enumerate() {
            assert_eq!(TrayCommand::SetTheme(theme).id(), Some(1400 + index as u16));
        }
        for (index, opacity) in OPACITY_VALUES.into_iter().enumerate() {
            assert_eq!(
                TrayCommand::SetOpacity(opacity).id(),
                Some(1420 + index as u16)
            );
        }

        let switch_bases = [1300, 1310, 1320];
        let display_bases = [1330, 1340, 1350];
        for (combo_index, combo) in HookKeyCombo::ALL.into_iter().enumerate() {
            for (mode_index, mode) in SWITCH_MODE_VALUES.into_iter().enumerate() {
                assert_eq!(
                    TrayCommand::SetSwitchMode { combo, mode }.id(),
                    Some(switch_bases[combo_index] + mode_index as u16)
                );
            }
            for (mode_index, mode) in DISPLAY_MODE_VALUES.into_iter().enumerate() {
                assert_eq!(
                    TrayCommand::SetDisplayMode { combo, mode }.id(),
                    Some(display_bases[combo_index] + mode_index as u16)
                );
            }
        }
    }

    #[test]
    fn invalid_and_gap_ids_are_rejected() {
        for id in [
            0,
            999,
            1005,
            1099,
            1105,
            1299,
            1303,
            1313,
            1323,
            1333,
            1343,
            1353,
            1404,
            1412,
            1419,
            1427,
            1502,
            1505,
            u16::MAX,
        ] {
            assert_eq!(TrayCommand::from_id(id), None, "ID {id} should be invalid");
        }
        assert_eq!(TrayCommand::SetOpacity(0).id(), None);
    }
}
