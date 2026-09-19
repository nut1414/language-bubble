//! Win32 and MSAA caret probes, including DPI conversion and height validation.
use super::{CaretQuality, ScreenPoint};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

const OBJID_CARET: i32 = -8;
const MIN_RELIABLE_CARET_HEIGHT_DIP: f32 = 8.0;

fn is_tall_caret(height: i32, dpi_scale: f32) -> bool {
    height as f32 >= MIN_RELIABLE_CARET_HEIGHT_DIP.max(MIN_RELIABLE_CARET_HEIGHT_DIP * dpi_scale)
}

pub(super) fn window_dpi_scale(hwnd: HWND) -> f32 {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        if dpi > 0 { dpi as f32 / 96.0 } else { 1.0 }
    }
}
pub(super) fn try_gui_thread_info() -> Option<ScreenPoint> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let thread_id = GetWindowThreadProcessId(hwnd, None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread_id, &mut gui).is_err() {
            return None;
        }
        if gui.hwndCaret.is_invalid() {
            return None;
        }
        let w = gui.rcCaret.right - gui.rcCaret.left;
        let h = gui.rcCaret.bottom - gui.rcCaret.top;
        if w <= 0 && h <= 0 {
            return None;
        }

        let mut pt = POINT {
            x: gui.rcCaret.left,
            y: gui.rcCaret.bottom,
        };
        let mut pt_top = POINT {
            x: gui.rcCaret.left,
            y: gui.rcCaret.top,
        };

        // Match target window's DPI awareness for ClientToScreen
        let target_ctx = GetWindowDpiAwarenessContext(gui.hwndCaret);
        let prev_ctx = SetThreadDpiAwarenessContext(target_ctx);
        let ok = ClientToScreen(gui.hwndCaret, &mut pt);
        let ok_top = ClientToScreen(gui.hwndCaret, &mut pt_top);
        SetThreadDpiAwarenessContext(prev_ctx);
        if !ok.as_bool() || !ok_top.as_bool() {
            return None;
        }

        // Convert to physical pixels. If either conversion fails, keep the
        // original logical height for reliability classification rather than
        // comparing logical pixels against a physical-DPI threshold.
        let physical_bottom =
            LogicalToPhysicalPointForPerMonitorDPI(Some(gui.hwndCaret), &mut pt).as_bool();
        let physical_top =
            LogicalToPhysicalPointForPerMonitorDPI(Some(gui.hwndCaret), &mut pt_top).as_bool();
        let height = if physical_bottom && physical_top {
            pt.y - pt_top.y
        } else {
            h
        };
        let dpi_scale = if physical_bottom && physical_top {
            window_dpi_scale(gui.hwndCaret)
        } else {
            1.0
        };
        let quality = if is_tall_caret(height, dpi_scale) {
            CaretQuality::Reliable
        } else {
            CaretQuality::Unreliable
        };

        Some(ScreenPoint::from_top_bottom(
            pt.x, pt_top.y, pt.y, quality, None,
        ))
    }
}

pub(super) fn try_msaa_caret() -> Option<ScreenPoint> {
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let thread_id = GetWindowThreadProcessId(hwnd, None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let _ = GetGUIThreadInfo(thread_id, &mut gui);

        let target = if !gui.hwndFocus.is_invalid() {
            gui.hwndFocus
        } else {
            hwnd
        };

        let iid = IAccessible::IID;
        let mut obj: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr =
            AccessibleObjectFromWindow(target, OBJID_CARET as u32, &iid as *const GUID, &mut obj);
        if hr.is_err() || obj.is_null() {
            return None;
        }

        // AccessibleObjectFromWindow transfers one COM reference to the caller.
        let acc = IAccessible::from_raw(obj);
        let mut left = 0i32;
        let mut top = 0i32;
        let mut width = 0i32;
        let mut height = 0i32;
        let child_var = VARIANT::from(0i32);
        let result = acc.accLocation(&mut left, &mut top, &mut width, &mut height, &child_var);

        // Restore DPI awareness
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        if result.is_err() || (left == 0 && top == 0 && width == 0 && height == 0) {
            return None;
        }

        let quality = if is_tall_caret(height, window_dpi_scale(target)) {
            CaretQuality::Reliable
        } else {
            CaretQuality::Unreliable
        };

        Some(ScreenPoint::from_top_bottom(
            left,
            top,
            top + height,
            quality,
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_height_threshold_keeps_normal_carets_reliable() {
        assert!(!is_tall_caret(2, 1.5));
        assert!(!is_tall_caret(11, 1.5));
        assert!(is_tall_caret(12, 1.5));
        assert!(is_tall_caret(17, 1.5));
        assert!(is_tall_caret(24, 1.5));
    }
}
