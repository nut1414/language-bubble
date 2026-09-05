use std::mem;

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use super::with_app;
use crate::types::HookKeyCombo;
use crate::types::SwitchMode;
use crate::{capslock, settings, tray};

pub(super) fn on_tray_right_click(hwnd: HWND) {
    let Some(snapshot) = with_app(|state| state.tray_menu_snapshot()) else {
        return;
    };
    let Some(cmd) = tray::show_context_menu(hwnd, &snapshot) else {
        return;
    };

    handle_menu_command(hwnd, cmd);
}

fn handle_menu_command(hwnd: HWND, cmd: tray::TrayCommand) {
    match cmd {
        tray::TrayCommand::PickCustomBackground => {
            let initial = with_app(|state| state.custom_colors.bg_color).unwrap_or(0);
            if let Some(new_color) = pick_color(hwnd, initial) {
                with_app(|state| {
                    state.custom_colors.bg_color = new_color;
                    state.bubble.set_custom_colors(state.custom_colors);
                    settings::report_result(
                        "save custom theme colors",
                        state
                            .settings
                            .save_custom_theme_colors(&state.custom_colors),
                    );
                });
            }
            return;
        }
        tray::TrayCommand::PickCustomForeground => {
            let initial = with_app(|state| state.custom_colors.fg_color).unwrap_or(0x00FFFFFF);
            if let Some(new_color) = pick_color(hwnd, initial) {
                with_app(|state| {
                    state.custom_colors.fg_color = new_color;
                    state.bubble.set_custom_colors(state.custom_colors);
                    settings::report_result(
                        "save custom theme colors",
                        state
                            .settings
                            .save_custom_theme_colors(&state.custom_colors),
                    );
                });
            }
            return;
        }
        _ => {}
    }

    with_app(|state| match cmd {
        tray::TrayCommand::Exit => {
            unsafe { PostQuitMessage(0) };
        }
        tray::TrayCommand::Feedback => unsafe {
            let _ = windows::Win32::UI::Shell::ShellExecuteW(
                None,
                w!("open"),
                w!("https://github.com/nut1414/language-bubble/issues"),
                None,
                None,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        },
        tray::TrayCommand::ToggleUpdateChecks => {
            let current = state.settings.check_for_updates();
            settings::report_result(
                "save update preference",
                state.settings.save_check_for_updates(!current),
            );
        }
        tray::TrayCommand::DownloadUpdate => unsafe {
            let _ = windows::Win32::UI::Shell::ShellExecuteW(
                None,
                w!("open"),
                w!("https://github.com/nut1414/language-bubble/releases/latest"),
                None,
                None,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        },
        tray::TrayCommand::ToggleStartWithWindows => {
            let current = settings::is_start_with_windows();
            settings::set_start_with_windows(!current);
        }
        tray::TrayCommand::ToggleHideOnTyping => {
            state.hide_on_typing = !state.hide_on_typing;
            settings::report_result(
                "save hide-on-typing preference",
                state.settings.save_hide_on_typing(state.hide_on_typing),
            );
        }
        tray::TrayCommand::ToggleExpandedMruOnly => {
            state.expanded_mru_only = !state.expanded_mru_only;
            settings::report_result(
                "save expanded MRU preference",
                state
                    .settings
                    .save_expanded_mru_only(state.expanded_mru_only),
            );
        }
        tray::TrayCommand::SetSize(size) => {
            state.bubble.set_size(size);
            settings::report_result("save bubble size", state.settings.save_bubble_size(size));
        }
        tray::TrayCommand::SetSwitchMode { combo, mode } => {
            state.bindings.set_switch_mode(combo, mode);
            state.hook.set_mode(combo, mode);
            settings::report_result(
                "save key switch mode",
                state.settings.save_key_switch_mode(combo, mode),
            );
            if combo == HookKeyCombo::CapsLock && mode != SwitchMode::Unused {
                capslock::ensure_caps_lock_off();
            }
        }
        tray::TrayCommand::SetDisplayMode { combo, mode } => {
            state.bindings.set_display_mode(combo, mode);
            settings::report_result(
                "save key display mode",
                state.settings.save_key_display_mode(combo, mode),
            );
        }
        tray::TrayCommand::SetTheme(mode) => {
            state.theme_mode = mode;
            state.bubble.set_theme_mode(mode);
            settings::report_result("save theme mode", state.settings.save_theme_mode(mode));
        }
        tray::TrayCommand::SetOpacity(opacity) => {
            state.custom_colors.opacity = opacity;
            state.bubble.set_custom_colors(state.custom_colors);
            settings::report_result(
                "save custom theme colors",
                state
                    .settings
                    .save_custom_theme_colors(&state.custom_colors),
            );
        }
        tray::TrayCommand::PickCustomBackground | tray::TrayCommand::PickCustomForeground => {
            unreachable!()
        }
    });
}

fn pick_color(hwnd: HWND, initial: u32) -> Option<u32> {
    use windows::Win32::UI::Controls::Dialogs::*;

    unsafe {
        let mut custom: [COLORREF; 16] = [COLORREF(0); 16];
        let mut cc = CHOOSECOLORW {
            lStructSize: mem::size_of::<CHOOSECOLORW>() as u32,
            hwndOwner: hwnd,
            rgbResult: COLORREF(initial),
            lpCustColors: custom.as_mut_ptr(),
            Flags: CC_RGBINIT | CC_FULLOPEN,
            ..Default::default()
        };
        if ChooseColorW(&mut cc).as_bool() {
            Some(cc.rgbResult.0)
        } else {
            None
        }
    }
}
