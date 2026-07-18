use std::cell::Cell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Error, HRESULT, Result};

use crate::types::*;

mod win_space;

use win_space::{WinKey, WinSpaceEvent, WinSpaceState};

/// Custom message posted to the main window when a switch key is pressed.
pub const WM_SWITCH_KEY: u32 = WM_APP + 1;
/// Custom message posted to the main window when any key is pressed.
pub const WM_ANY_KEY: u32 = WM_APP + 2;

const SELF_INJECTED_TAG: usize = 0x4C42;
const VK_CAPITAL_U16: u16 = 0x14;
const E_UNEXPECTED: HRESULT = HRESULT(0x8000FFFFu32 as i32);

// Global state for the hook callback (must be static since the callback is a C function pointer)
static SUPPRESS_SELF: AtomicBool = AtomicBool::new(false);

struct HookState {
    target_hwnd: HWND,
    bindings: KeyBindings,
    win_space: WinSpaceState,
    alt_held: bool,
    shift_held: bool,
    caps_held: bool,
    alt_shift_primed: bool,
    alt_shift_consumed: bool,
}

impl HookState {
    fn new(target_hwnd: HWND, bindings: KeyBindings) -> Self {
        Self {
            target_hwnd,
            bindings,
            win_space: WinSpaceState::default(),
            alt_held: false,
            shift_held: false,
            caps_held: false,
            alt_shift_primed: false,
            alt_shift_consumed: false,
        }
    }

    fn set_mode(&mut self, combo: HookKeyCombo, mode: SwitchMode) {
        self.bindings.set_switch_mode(combo, mode);
    }
}

thread_local! {
    static HOOK: Cell<Option<NonNull<HookState>>> = const { Cell::new(None) };
}

pub struct InstalledHook {
    handle: HHOOK,
    state: Box<HookState>,
}

pub fn set_suppress_self_generated(suppress: bool) {
    SUPPRESS_SELF.store(suppress, Ordering::SeqCst);
}

impl InstalledHook {
    pub fn install(target_hwnd: HWND, bindings: &KeyBindings) -> Result<Self> {
        if HOOK.with(|cell| cell.get().is_some()) {
            return Err(Error::new(E_UNEXPECTED, "keyboard hook already installed"));
        }

        let mut state = Box::new(HookState::new(target_hwnd, *bindings));
        let module = unsafe { GetModuleHandleW(None).unwrap_or_default() };
        let handle =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), Some(module.into()), 0)? };
        let state_pointer = NonNull::from(state.as_mut());
        HOOK.with(|cell| cell.set(Some(state_pointer)));
        Ok(Self { handle, state })
    }

    pub fn set_mode(&mut self, combo: HookKeyCombo, mode: SwitchMode) {
        self.state.set_mode(combo, mode);
    }
}

impl Drop for InstalledHook {
    fn drop(&mut self) {
        HOOK.with(|cell| {
            let pointer = NonNull::from(self.state.as_mut());
            if cell.get() == Some(pointer) {
                cell.set(None);
            }
        });
        unsafe {
            let _ = UnhookWindowsHookEx(self.handle);
        }
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code < 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        let state_ptr = HOOK.with(|cell| cell.get());
        let Some(mut ptr) = state_ptr else {
            return CallNextHookEx(None, code, wparam, lparam);
        };
        let state = ptr.as_mut();

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

        // --- Win + Space ---
        let win_space_event = match vk {
            v if v == VK_LWIN.0 && is_down => Some(WinSpaceEvent::WinDown(WinKey::Left)),
            v if v == VK_LWIN.0 && is_up => Some(WinSpaceEvent::WinUp(WinKey::Left)),
            v if v == VK_RWIN.0 && is_down => Some(WinSpaceEvent::WinDown(WinKey::Right)),
            v if v == VK_RWIN.0 && is_up => Some(WinSpaceEvent::WinUp(WinKey::Right)),
            v if v == VK_SPACE.0 && is_down => Some(WinSpaceEvent::SpaceDown),
            v if v == VK_SPACE.0 && is_up => Some(WinSpaceEvent::SpaceUp),
            _ => None,
        };

        if let Some(event) = win_space_event
            && let Some(decision) = state.win_space.handle(
                event,
                state.bindings.get(HookKeyCombo::WinSpace).switch_mode != SwitchMode::Unused,
            )
        {
            if decision.switch_layout {
                if state.alt_held && state.shift_held {
                    state.alt_shift_consumed = true;
                }
                let _ = PostMessageW(
                    Some(state.target_hwnd),
                    WM_SWITCH_KEY,
                    WPARAM(HookKeyCombo::WinSpace as usize),
                    LPARAM(0),
                );
            }

            if decision.neutralize_start {
                // Suppress the real Win key-up only when the full Ctrl tap and
                // synthetic Win-up sequence was injected successfully.
                if inject_win_combo_release(vk) {
                    return LRESULT(1);
                }
                return CallNextHookEx(None, code, wparam, lparam);
            }

            if decision.suppress {
                return LRESULT(1);
            }
            return CallNextHookEx(None, code, wparam, lparam);
        }

        // --- Alt key ---
        if vk == VK_LMENU.0 || vk == VK_RMENU.0 || vk == VK_MENU.0 {
            if is_down {
                let was_held = state.alt_held;
                state.alt_held = true;
                if !was_held && state.shift_held {
                    state.alt_shift_primed = true;
                    state.alt_shift_consumed = false;
                }
            } else if is_up {
                state.alt_held = false;
                if state.alt_shift_primed
                    && !state.alt_shift_consumed
                    && state.bindings.get(HookKeyCombo::AltShift).switch_mode != SwitchMode::Unused
                {
                    let _ = inject_ctrl_tap();
                    let _ = PostMessageW(
                        Some(state.target_hwnd),
                        WM_SWITCH_KEY,
                        WPARAM(HookKeyCombo::AltShift as usize),
                        LPARAM(0),
                    );
                }
                state.alt_shift_primed = false;
                state.alt_shift_consumed = false;
            }
            return CallNextHookEx(None, code, wparam, lparam);
        }

        // --- Shift key ---
        if vk == VK_LSHIFT.0 || vk == VK_RSHIFT.0 || vk == VK_SHIFT.0 {
            if is_down {
                let was_held = state.shift_held;
                state.shift_held = true;
                if !was_held && state.alt_held {
                    state.alt_shift_primed = true;
                    state.alt_shift_consumed = false;
                }
            } else if is_up {
                state.shift_held = false;
                if state.alt_shift_primed
                    && !state.alt_shift_consumed
                    && state.bindings.get(HookKeyCombo::AltShift).switch_mode != SwitchMode::Unused
                {
                    let _ = inject_ctrl_tap();
                    let _ = PostMessageW(
                        Some(state.target_hwnd),
                        WM_SWITCH_KEY,
                        WPARAM(HookKeyCombo::AltShift as usize),
                        LPARAM(0),
                    );
                }
                state.alt_shift_primed = false;
                state.alt_shift_consumed = false;
            }
            return CallNextHookEx(None, code, wparam, lparam);
        }

        // --- CapsLock ---
        if vk == VK_CAPITAL_U16 {
            if state.bindings.get(HookKeyCombo::CapsLock).switch_mode == SwitchMode::Unused {
                return CallNextHookEx(None, code, wparam, lparam);
            }
            if is_down && !state.caps_held {
                state.caps_held = true;
                if state.alt_held && state.shift_held {
                    state.alt_shift_consumed = true;
                }
                let _ = PostMessageW(
                    Some(state.target_hwnd),
                    WM_SWITCH_KEY,
                    WPARAM(HookKeyCombo::CapsLock as usize),
                    LPARAM(0),
                );
            } else if is_up {
                state.caps_held = false;
            }
            return LRESULT(1); // Suppress both down and up
        }

        // --- All other keys ---
        if is_down {
            if state.alt_held && state.shift_held {
                state.alt_shift_consumed = true;
            }
            let _ = PostMessageW(Some(state.target_hwnd), WM_ANY_KEY, WPARAM(0), LPARAM(0));
        }

        CallNextHookEx(None, code, wparam, lparam)
    }
}

fn keyboard_input(vk: u16, down: bool) -> INPUT {
    let flags = if down {
        KEYBD_EVENT_FLAGS(0)
    } else {
        KEYEVENTF_KEYUP
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SELF_INJECTED_TAG,
            },
        },
    }
}

unsafe fn send_inputs(inputs: &[INPUT]) -> bool {
    unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) == inputs.len() as u32 }
}

unsafe fn inject_win_combo_release(vk: u16) -> bool {
    let inputs = [
        keyboard_input(VK_CONTROL.0, true),
        keyboard_input(VK_CONTROL.0, false),
        keyboard_input(vk, false),
    ];
    let complete = unsafe { send_inputs(&inputs) };
    if !complete {
        // If SendInput accepted only a prefix, make a best-effort attempt to
        // release both modifiers before passing the real Win-up through.
        let releases = [
            keyboard_input(VK_CONTROL.0, false),
            keyboard_input(vk, false),
        ];
        let _ = unsafe { send_inputs(&releases) };
    }
    complete
}

unsafe fn inject_ctrl_tap() -> bool {
    let inputs = [
        keyboard_input(VK_CONTROL.0, true),
        keyboard_input(VK_CONTROL.0, false),
    ];
    unsafe { send_inputs(&inputs) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_state_starts_released_with_configured_bindings() {
        let bindings = KeyBindings::default();
        let state = HookState::new(HWND::default(), bindings);

        assert_eq!(state.bindings, bindings);
        assert!(!state.alt_held);
        assert!(!state.shift_held);
        assert!(!state.caps_held);
        assert!(!state.alt_shift_primed);
        assert!(!state.alt_shift_consumed);
    }

    #[test]
    fn hook_state_updates_only_selected_mode() {
        let mut state = HookState::new(HWND::default(), KeyBindings::default());
        state.set_mode(HookKeyCombo::WinSpace, SwitchMode::Mru);

        assert_eq!(
            state.bindings.get(HookKeyCombo::WinSpace).switch_mode,
            SwitchMode::Mru
        );
        assert_eq!(
            state.bindings.get(HookKeyCombo::CapsLock).switch_mode,
            SwitchMode::AllLanguage
        );
    }
}
