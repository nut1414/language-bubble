use std::cell::Cell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Error, HRESULT, Result};

use crate::types::*;

mod policy;
use policy::{Effect, HookPolicy, KeyEvent, KeyTransition};

/// Custom message posted to the main window when a switch key is pressed.
pub const WM_SWITCH_KEY: u32 = WM_APP + 1;
/// Custom message posted to the main window when any key is pressed.
pub const WM_ANY_KEY: u32 = WM_APP + 2;

const SELF_INJECTED_TAG: usize = 0x4C42;
const E_UNEXPECTED: HRESULT = HRESULT(0x8000FFFFu32 as i32);

// Global state for the hook callback (must be static since the callback is a C function pointer)
static SUPPRESS_SELF: AtomicBool = AtomicBool::new(false);

struct HookState {
    target_hwnd: HWND,
    policy: HookPolicy,
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

        let mut state = Box::new(HookState {
            target_hwnd,
            policy: HookPolicy::new(*bindings),
        });
        let module = unsafe { GetModuleHandleW(None).unwrap_or_default() };
        let handle =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), Some(module.into()), 0)? };
        let state_pointer = NonNull::from(state.as_mut());
        HOOK.with(|cell| cell.set(Some(state_pointer)));
        Ok(Self { handle, state })
    }

    pub fn set_mode(&mut self, combo: HookKeyCombo, mode: SwitchMode) {
        self.state.policy.set_mode(combo, mode);
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
        let Some(mut ptr) = HOOK.with(|cell| cell.get()) else {
            return CallNextHookEx(None, code, wparam, lparam);
        };
        if SUPPRESS_SELF.load(Ordering::SeqCst) {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let kbd = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if kbd.dwExtraInfo == SELF_INJECTED_TAG {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let transition = if wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize {
            KeyTransition::Down
        } else if wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize {
            KeyTransition::Up
        } else {
            KeyTransition::Other
        };
        // No mutable state reference survives into SendInput/PostMessage. Tagged
        // synthetic callbacks bypass the policy before borrowing it at all.
        let (target_hwnd, decision) = {
            let state = ptr.as_mut();
            (
                state.target_hwnd,
                state.policy.handle(KeyEvent {
                    vk: kbd.vkCode as u16,
                    transition,
                }),
            )
        };
        let consume = decision.execute(|effect| match effect {
            Effect::Switch(combo) => PostMessageW(
                Some(target_hwnd),
                WM_SWITCH_KEY,
                WPARAM(combo as usize),
                LPARAM(0),
            )
            .is_ok(),
            Effect::AnyKey => {
                PostMessageW(Some(target_hwnd), WM_ANY_KEY, WPARAM(0), LPARAM(0)).is_ok()
            }
            Effect::ReleaseWin(vk) => inject_win_combo_release(vk),
            Effect::CtrlTap => inject_ctrl_tap(),
        });
        if consume {
            LRESULT(1)
        } else {
            CallNextHookEx(None, code, wparam, lparam)
        }
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
