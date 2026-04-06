use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::types::*;

/// Custom message posted to the main window when a switch key is pressed.
pub const WM_SWITCH_KEY: u32 = WM_APP + 1;
/// Custom message posted to the main window when any key is pressed.
pub const WM_ANY_KEY: u32 = WM_APP + 2;

const SELF_INJECTED_TAG: usize = 0x4C42;
const VK_CAPITAL_U16: u16 = 0x14;

// Global state for the hook callback (must be static since the callback is a C function pointer)
static SUPPRESS_SELF: AtomicBool = AtomicBool::new(false);

struct HookState {
    hhook: HHOOK,
    target_hwnd: HWND,
    caps_lock_mode: SwitchMode,
    win_space_mode: SwitchMode,
    alt_shift_mode: SwitchMode,
    win_held: bool,
    win_used_for_combo: bool,
    alt_held: bool,
    shift_held: bool,
}

thread_local! {
    static HOOK: Cell<Option<*mut HookState>> = const { Cell::new(None) };
}

pub fn set_suppress_self_generated(suppress: bool) {
    SUPPRESS_SELF.store(suppress, Ordering::SeqCst);
}

pub fn install(
    target_hwnd: HWND,
    caps_lock_mode: SwitchMode,
    win_space_mode: SwitchMode,
    alt_shift_mode: SwitchMode,
) {
    unsafe {
        let hmod = GetModuleHandleW(None).unwrap_or_default();
        let hhook =
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), Some(hmod.into()), 0).unwrap();
        let state = Box::new(HookState {
            hhook,
            target_hwnd,
            caps_lock_mode,
            win_space_mode,
            alt_shift_mode,
            win_held: false,
            win_used_for_combo: false,
            alt_held: false,
            shift_held: false,
        });
        HOOK.set(Some(Box::into_raw(state)));
    }
}

pub fn uninstall() {
    HOOK.with(|cell| {
        if let Some(ptr) = cell.take() {
            unsafe {
                let state = Box::from_raw(ptr);
                let _ = UnhookWindowsHookEx(state.hhook);
            }
        }
    });
}

pub fn set_caps_lock_mode(mode: SwitchMode) {
    with_state(|s| s.caps_lock_mode = mode);
}

pub fn set_win_space_mode(mode: SwitchMode) {
    with_state(|s| s.win_space_mode = mode);
}

pub fn set_alt_shift_mode(mode: SwitchMode) {
    with_state(|s| s.alt_shift_mode = mode);
}

fn with_state<F: FnOnce(&mut HookState)>(f: F) {
    HOOK.with(|cell| {
        if let Some(ptr) = cell.get() {
            unsafe { f(&mut *ptr) }
        }
    });
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let state_ptr = HOOK.with(|cell| cell.get());
    let Some(ptr) = state_ptr else {
        return CallNextHookEx(None, code, wparam, lparam);
    };
    let state = &mut *ptr;

    if SUPPRESS_SELF.load(Ordering::SeqCst) {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);

    // Skip self-injected events
    if kbd.dwExtraInfo == SELF_INJECTED_TAG {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let vk = kbd.vkCode as u16;
    let is_down = wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize;
    let is_up = wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize;

    // --- Windows key ---
    if vk == VK_LWIN.0 || vk == VK_RWIN.0 {
        if is_down {
            state.win_held = true;
            state.win_used_for_combo = false;
        } else if is_up {
            state.win_held = false;
            if state.win_used_for_combo {
                state.win_used_for_combo = false;
                // Suppress real Win key-up, inject Ctrl tap + synthetic Win up
                // to prevent Start menu from opening
                inject_ctrl_tap();
                inject_key(vk, false, true); // synthetic Win up
                return LRESULT(1);
            }
        }
        return CallNextHookEx(None, code, wparam, lparam);
    }

    // --- Space (when Win held) ---
    if vk == VK_SPACE.0 && state.win_held && state.win_space_mode != SwitchMode::Unused {
        if is_down {
            state.win_used_for_combo = true;
            let _ = PostMessageW(
                Some(state.target_hwnd),
                WM_SWITCH_KEY,
                WPARAM(HookKeyCombo::WinSpace as usize),
                LPARAM(0),
            );
        }
        return LRESULT(1); // Suppress both down and up
    }

    // --- Alt key ---
    if vk == VK_LMENU.0 || vk == VK_RMENU.0 || vk == VK_MENU.0 {
        if is_down {
            state.alt_held = true;
            if state.shift_held && state.alt_shift_mode != SwitchMode::Unused {
                let _ = PostMessageW(
                    Some(state.target_hwnd),
                    WM_SWITCH_KEY,
                    WPARAM(HookKeyCombo::AltShift as usize),
                    LPARAM(0),
                );
                return LRESULT(1);
            }
        } else if is_up {
            state.alt_held = false;
            // Always pass through Alt key-up to prevent stuck key
        }
        return CallNextHookEx(None, code, wparam, lparam);
    }

    // --- Shift key ---
    if vk == VK_LSHIFT.0 || vk == VK_RSHIFT.0 || vk == VK_SHIFT.0 {
        if is_down {
            state.shift_held = true;
            if state.alt_held && state.alt_shift_mode != SwitchMode::Unused {
                let _ = PostMessageW(
                    Some(state.target_hwnd),
                    WM_SWITCH_KEY,
                    WPARAM(HookKeyCombo::AltShift as usize),
                    LPARAM(0),
                );
                return LRESULT(1);
            }
        } else if is_up {
            state.shift_held = false;
            // Always pass through Shift key-up to prevent stuck key
        }
        return CallNextHookEx(None, code, wparam, lparam);
    }

    // --- CapsLock ---
    if vk == VK_CAPITAL_U16 {
        if state.caps_lock_mode == SwitchMode::Unused {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        if is_down {
            let _ = PostMessageW(
                Some(state.target_hwnd),
                WM_SWITCH_KEY,
                WPARAM(HookKeyCombo::CapsLock as usize),
                LPARAM(0),
            );
        }
        return LRESULT(1); // Suppress both down and up
    }

    // --- All other keys ---
    if is_down {
        let _ = PostMessageW(
            Some(state.target_hwnd),
            WM_ANY_KEY,
            WPARAM(0),
            LPARAM(0),
        );
    }

    CallNextHookEx(None, code, wparam, lparam)
    }
}

unsafe fn inject_key(vk: u16, down: bool, tagged: bool) {
    unsafe {
        let flags = if down {
            KEYBD_EVENT_FLAGS(0)
        } else {
            KEYEVENTF_KEYUP
        };
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: if tagged { SELF_INJECTED_TAG } else { 0 },
                },
            },
        };
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

unsafe fn inject_ctrl_tap() {
    unsafe {
        inject_key(VK_CONTROL.0, true, true);
        inject_key(VK_CONTROL.0, false, true);
    }
}
