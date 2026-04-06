use windows::Win32::UI::Input::KeyboardAndMouse::*;

use crate::hook;

const VK_CAPITAL_U8: u8 = 0x14;

pub fn ensure_caps_lock_off() {
    unsafe {
        let state = GetKeyState(VK_CAPITAL.0 as i32);
        if state & 1 == 0 {
            return; // Already off
        }
        hook::set_suppress_self_generated(true);
        keybd_event(VK_CAPITAL_U8, 0x45, KEYEVENTF_EXTENDEDKEY, 0);
        keybd_event(
            VK_CAPITAL_U8,
            0x45,
            KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP,
            0,
        );
        hook::set_suppress_self_generated(false);
    }
}
