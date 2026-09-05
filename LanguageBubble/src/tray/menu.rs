use super::command::command_id;
use super::{TrayCommand, TrayMenuSnapshot};
use crate::types::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

/// Until attached, a submenu owns its handle; Windows owns attached children.
struct MenuOwner(HMENU);
impl MenuOwner {
    fn new() -> Option<Self> {
        unsafe { CreatePopupMenu().ok().map(Self) }
    }
}
impl Drop for MenuOwner {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}
fn attach_submenu(parent: HMENU, child: MenuOwner, flags: MENU_ITEM_FLAGS, label: PCWSTR) {
    if unsafe { AppendMenuW(parent, flags, child.0.0 as usize, label) }.is_ok() {
        std::mem::forget(child);
    }
}

pub fn show_context_menu(hwnd: HWND, snapshot: &TrayMenuSnapshot) -> Option<TrayCommand> {
    let owner = build_menu(snapshot)?;
    let menu = owner.0;
    unsafe {
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
        drop(owner);

        if cmd.0 != 0 {
            TrayCommand::from_id(cmd.0 as u16)
        } else {
            None
        }
    }
}

/// Construct handles without showing UI, so native menu structure is testable.
fn build_menu(snapshot: &TrayMenuSnapshot) -> Option<MenuOwner> {
    let owner = MenuOwner::new()?;
    let menu = owner.0;
    add_languages(menu, snapshot)?;
    add_startup(menu, snapshot)?;
    add_appearance(menu, snapshot)?;
    add_bindings(menu, snapshot)?;
    add_preferences(menu, snapshot)?;
    add_advanced(menu, snapshot)?;
    add_actions(menu, snapshot)?;
    Some(owner)
}

fn add_languages(menu: HMENU, snapshot: &TrayMenuSnapshot) -> Option<()> {
    let layouts = &snapshot.layouts;
    let current_hkl = snapshot.current_hkl;
    unsafe {
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

        Some(())
    }
}

fn add_startup(menu: HMENU, snapshot: &TrayMenuSnapshot) -> Option<()> {
    let start_with_windows = snapshot.start_with_windows;
    unsafe {
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

        Some(())
    }
}

fn add_appearance(menu: HMENU, snapshot: &TrayMenuSnapshot) -> Option<()> {
    let size = snapshot.size;
    let theme_mode = snapshot.theme_mode;
    let custom_colors = &snapshot.custom_colors;
    unsafe {
        // Size submenu
        let size_menu_owner = MenuOwner::new()?;
        let size_menu = size_menu_owner.0;
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
        attach_submenu(menu, size_menu_owner, MF_POPUP, w!("Size"));

        let theme_menu_owner = MenuOwner::new()?;
        let theme_menu = theme_menu_owner.0;
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

        let customize_menu_owner = MenuOwner::new()?;
        let customize_menu = customize_menu_owner.0;

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

        let opacity_menu_owner = MenuOwner::new()?;
        let opacity_menu = opacity_menu_owner.0;
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
        attach_submenu(
            customize_menu,
            opacity_menu_owner,
            MF_POPUP,
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
        attach_submenu(
            theme_menu,
            customize_menu_owner,
            customize_flags,
            PCWSTR(customize_label.as_ptr()),
        );

        attach_submenu(menu, theme_menu_owner, MF_POPUP, w!("Theme"));

        Some(())
    }
}

fn add_bindings(menu: HMENU, snapshot: &TrayMenuSnapshot) -> Option<()> {
    let bindings = snapshot.bindings;
    unsafe {
        // Key Bindings submenu (now includes display mode per key)
        let key_menu_owner = MenuOwner::new()?;
        let key_menu = key_menu_owner.0;
        for combo in HookKeyCombo::ALL {
            let _ = add_key_submenu(key_menu, combo, bindings.get(combo));
        }
        attach_submenu(menu, key_menu_owner, MF_POPUP, w!("Key Bindings"));

        Some(())
    }
}

fn add_preferences(menu: HMENU, snapshot: &TrayMenuSnapshot) -> Option<()> {
    let hide_on_typing = snapshot.hide_on_typing;
    let expanded_mru_only = snapshot.expanded_mru_only;
    unsafe {
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

        Some(())
    }
}

fn add_advanced(menu: HMENU, snapshot: &TrayMenuSnapshot) -> Option<()> {
    let app_version = snapshot.app_version;
    let is_msix = snapshot.is_msix;
    let check_for_updates = snapshot.check_for_updates;
    let pending_update = snapshot.pending_update.as_deref();
    unsafe {
        // Advanced submenu
        let advanced_menu_owner = MenuOwner::new()?;
        let advanced_menu = advanced_menu_owner.0;
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

        attach_submenu(menu, advanced_menu_owner, MF_POPUP, w!("Advanced"));

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

        Some(())
    }
}

fn add_actions(menu: HMENU, _snapshot: &TrayMenuSnapshot) -> Option<()> {
    unsafe {
        // Feedback
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            command_id(TrayCommand::Feedback),
            w!("Feedback"),
        );

        // Exit
        let _ = AppendMenuW(menu, MF_STRING, command_id(TrayCommand::Exit), w!("Exit"));

        Some(())
    }
}

unsafe fn add_key_submenu(
    parent: HMENU,
    combo: HookKeyCombo,
    binding: KeyBindingConfig,
) -> Option<()> {
    unsafe {
        let sub_owner = MenuOwner::new()?;
        let sub = sub_owner.0;
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
        attach_submenu(parent, sub_owner, MF_POPUP, PCWSTR(wide.as_ptr()));
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> TrayMenuSnapshot {
        TrayMenuSnapshot {
            layouts: Vec::new(),
            current_hkl: None,
            start_with_windows: true,
            size: BubbleSize::Large,
            bindings: KeyBindings::default(),
            hide_on_typing: true,
            expanded_mru_only: false,
            theme_mode: ThemeMode::Custom,
            custom_colors: CustomThemeColors::default(),
            check_for_updates: true,
            pending_update: Some("999.0.0".into()),
            app_version: "0.5.0",
            is_msix: false,
        }
    }

    #[test]
    fn native_menu_matches_snapshot_and_releases_owned_submenus() {
        // Create/query menu handles only: no foreground window or popup display.
        let snapshot = snapshot();
        let menu = build_menu(&snapshot).expect("menu");
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 13);
            assert_ne!(GetMenuState(menu.0, 3, MF_BYPOSITION) & MF_CHECKED.0, 0);
            assert_ne!(GetMenuState(menu.0, 7, MF_BYPOSITION) & MF_CHECKED.0, 0);
            assert_eq!(GetMenuState(menu.0, 8, MF_BYPOSITION) & MF_CHECKED.0, 0);
            let size = GetSubMenu(menu.0, 4);
            assert_ne!(GetMenuState(size, 3, MF_BYPOSITION) & MF_CHECKED.0, 0);
            let theme = GetSubMenu(menu.0, 5);
            assert_ne!(GetMenuState(theme, 3, MF_BYPOSITION) & MF_CHECKED.0, 0);
            assert_eq!(GetMenuState(theme, 5, MF_BYPOSITION) & MF_GRAYED.0, 0);
            let keys = GetSubMenu(menu.0, 6);
            let caps = GetSubMenu(keys, 0);
            assert_ne!(GetMenuState(caps, 0, MF_BYPOSITION) & MF_CHECKED.0, 0);
            assert_ne!(GetMenuState(caps, 4, MF_BYPOSITION) & MF_CHECKED.0, 0);
            let advanced = GetSubMenu(menu.0, 9);
            assert_eq!(GetMenuItemCount(Some(advanced)), 4);
            assert_ne!(GetMenuState(advanced, 2, MF_BYPOSITION) & MF_CHECKED.0, 0);
            drop(menu);
            assert!(!IsMenu(size).as_bool());
            assert!(!IsMenu(theme).as_bool());
            assert!(!IsMenu(caps).as_bool());
            assert!(!IsMenu(advanced).as_bool());
        }
    }

    #[test]
    fn packaged_and_pending_update_visibility_remain_distinct() {
        for (packaged, pending, expected_count) in [
            (true, true, 1),
            (true, false, 1),
            (false, true, 4),
            (false, false, 3),
        ] {
            let mut snapshot = snapshot();
            snapshot.is_msix = packaged;
            snapshot.theme_mode = ThemeMode::System;
            if !pending {
                snapshot.pending_update = None;
            }
            let menu = build_menu(&snapshot).expect("menu");
            unsafe {
                assert_eq!(
                    GetMenuItemCount(Some(GetSubMenu(menu.0, 9))),
                    expected_count
                );
                assert_ne!(
                    GetMenuState(GetSubMenu(menu.0, 5), 5, MF_BYPOSITION) & MF_GRAYED.0,
                    0
                );
            }
        }
    }
}
