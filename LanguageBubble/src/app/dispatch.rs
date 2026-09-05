use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use super::menu::on_tray_right_click;
use super::{AppState, DEFERRED, with_app};
use crate::types::*;
use crate::{bubble, capslock, caret, hook, language, tray, update};

const WM_SETTINGCHANGE: u32 = 0x001A;

pub(super) unsafe extern "system" fn msg_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            m if m == hook::WM_SWITCH_KEY => {
                let Ok(combo) = HookKeyCombo::try_from(wparam.0) else {
                    return LRESULT(0);
                };
                on_switch_key(combo);
                LRESULT(0)
            }
            m if m == hook::WM_ANY_KEY => {
                with_app(|state| {
                    if state.hide_on_typing {
                        state.bubble.instant_hide();
                    }
                });
                LRESULT(0)
            }
            m if m == tray::WM_TRAY_CALLBACK => {
                let mouse_msg = (lparam.0 & 0xFFFF) as u32;
                if mouse_msg == WM_RBUTTONUP {
                    on_tray_right_click(hwnd);
                }
                LRESULT(0)
            }
            m if m == update::WM_UPDATE_AVAILABLE => {
                on_update_available();
                LRESULT(0)
            }
            WM_TIMER => {
                let timer_id = wparam.0;
                with_app(|state| match timer_id {
                    bubble::TIMER_HIDE => {
                        let _ = KillTimer(Some(hwnd), bubble::TIMER_HIDE);
                        state.bubble.begin_hide();
                    }
                    bubble::TIMER_TOPMOST => {
                        state.bubble.refresh_topmost();
                    }
                    bubble::TIMER_ANIM => {
                        state.bubble.tick();
                    }
                    _ => {}
                });
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                if is_system_theme_change(lparam) {
                    with_app(|state| {
                        state.bubble.refresh_theme();
                    });
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub(super) fn on_update_available() {
    let handled = with_app(|state| {
        let new_version = {
            if let Ok(mut guard) = update::PENDING_UPDATE.lock() {
                guard.take()
            } else {
                None
            }
        };
        let Some(version) = new_version else {
            return;
        };
        state.pending_update = Some(version.clone());
        state
            ._tray
            .show_balloon("Language Bubble", &format!("Update available: {}", version));
    });
    if handled.is_none() {
        // The pending version remains in the mutex until AppState is available.
        DEFERRED.with(|events| events.defer_update());
    }
}

/// Returns whether `lparam` names the Windows immersive color setting.
///
/// # Safety
///
/// For `WM_SETTINGCHANGE`, Windows guarantees that a non-null `lparam` points
/// to a null-terminated UTF-16 string for the duration of the callback.
unsafe fn is_system_theme_change(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }

    let setting_name = PCWSTR(lparam.0 as *const u16);
    unsafe {
        setting_name
            .to_string()
            .is_ok_and(|name| name == "ImmersiveColorSet")
    }
}

fn on_switch_key(combo: HookKeyCombo) {
    let handled = with_app(|state| {
        if state.is_switching {
            state.pending_combo = Some(combo);
            return;
        }
        state.is_switching = true;
        process_switch(state, combo);
    });
    if handled.is_none() {
        DEFERRED.with(|events| events.defer_switch(combo));
    }
}

fn process_switch(state: &mut AppState, combo: HookKeyCombo) {
    let binding = state.bindings.get(combo);
    let mode = binding.switch_mode;
    if mode == SwitchMode::Unused {
        state.is_switching = false;
        return;
    }

    // Input methods can be added or removed while the utility is running.
    state.language_service.refresh_layouts();

    // Set display mode for this key binding
    state.bubble.set_display_mode(binding.display_mode);

    // Ensure CapsLock stays off
    if combo == HookKeyCombo::CapsLock {
        capslock::ensure_caps_lock_off();
    }

    // Record current layout before switching
    if let Some(before) = state.language_service.get_current_layout() {
        let hkl = before.hkl;
        state.language_service.record_layout_usage(hkl);
    }

    // Switch language
    let new_layout = if mode == SwitchMode::Mru {
        state.language_service.switch_to_mru().cloned()
    } else {
        state.language_service.switch_to_next().cloned()
    };

    let Some(new_layout) = new_layout else {
        state.is_switching = false;
        return;
    };

    state.language_service.record_layout_usage(new_layout.hkl);

    // Get caret position
    let caret_pos = caret::get_caret_screen_position();

    // Pick which layouts to show
    let display_layouts: Vec<language::LayoutInfo> = if mode == SwitchMode::AllLanguage {
        state.language_service.layouts().to_vec()
    } else if state.expanded_mru_only && state.bubble.display_mode() == DisplayMode::Expanded {
        state.language_service.get_mru_layouts()
    } else {
        state.language_service.layouts().to_vec()
    };

    let selected_index = display_layouts
        .iter()
        .position(|l| l.hkl == new_layout.hkl)
        .unwrap_or(0) as i32;

    state
        .bubble
        .show_bubble(&display_layouts, selected_index, caret_pos);

    // Process pending
    let pending = DEFERRED.with(|events| events.take_switch_after(&mut state.pending_combo));
    if let Some(next_combo) = pending {
        process_switch(state, next_combo);
    } else {
        state.is_switching = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_switch_message_is_ignored_before_accessing_app_state() {
        for value in [3, usize::MAX] {
            assert_eq!(
                unsafe {
                    msg_wnd_proc(
                        HWND::default(),
                        hook::WM_SWITCH_KEY,
                        WPARAM(value),
                        LPARAM(0),
                    )
                },
                LRESULT(0)
            );
        }
    }

    #[test]
    fn system_theme_change_compares_setting_contents() {
        let immersive: Vec<u16> = "ImmersiveColorSet"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let unrelated: Vec<u16> = "intl".encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            assert!(is_system_theme_change(LPARAM(immersive.as_ptr() as isize)));
            assert!(!is_system_theme_change(LPARAM(unrelated.as_ptr() as isize)));
            assert!(!is_system_theme_change(LPARAM(0)));
        }
    }
}
