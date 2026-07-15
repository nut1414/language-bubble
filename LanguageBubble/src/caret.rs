use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, SAFEARRAY};
use windows::Win32::System::Ole::{
    SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
const OBJID_CARET: i32 = -8;
const MAX_BOUNDING_RECT_VALUES: usize = 4096;

#[derive(Debug, Clone, Copy)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,         // caret bottom (for placing bubble below)
    pub caret_top: i32, // caret top (for placing bubble above)
}

pub fn get_caret_screen_position() -> Option<ScreenPoint> {
    // Strategy 1: Win32 GetGUIThreadInfo (Notepad, classic Win32 apps)
    if let Some(pt) = try_gui_thread_info() {
        return Some(pt);
    }

    // Strategy 2: MSAA IAccessible OBJID_CARET (Chrome, many apps)
    if let Some(pt) = try_msaa_caret() {
        return Some(pt);
    }

    // Strategy 3+4: COM UI Automation (Explorer, modern controls, Office, Edge)
    if let Some(pt) = try_uia_caret() {
        return Some(pt);
    }

    None
}

fn try_gui_thread_info() -> Option<ScreenPoint> {
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

        // Convert to physical pixels
        let _ = LogicalToPhysicalPointForPerMonitorDPI(Some(gui.hwndCaret), &mut pt);
        let _ = LogicalToPhysicalPointForPerMonitorDPI(Some(gui.hwndCaret), &mut pt_top);

        Some(ScreenPoint {
            x: pt.x,
            y: pt.y,
            caret_top: pt_top.y,
        })
    }
}

fn try_msaa_caret() -> Option<ScreenPoint> {
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 as _,
        ));

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
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 as _,
        ));

        if result.is_err() || (left == 0 && top == 0 && width == 0 && height == 0) {
            return None;
        }

        Some(ScreenPoint {
            x: left,
            y: top + height,
            caret_top: top,
        })
    }
}

fn try_uia_caret() -> Option<ScreenPoint> {
    // Wrap in catch_unwind because COM UIA calls can sometimes fail unexpectedly
    std::panic::catch_unwind(try_uia_caret_inner).ok().flatten()
}

fn try_uia_caret_inner() -> Option<ScreenPoint> {
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 as _,
        ));

        let uia: IUIAutomation =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()?;
        let focused = uia.GetFocusedElement().ok()?;

        // Try focused element directly
        if let Some(pt) = try_uia_element_caret(&focused) {
            return Some(pt);
        }

        // Walk up the tree (max 4 levels)
        if let Ok(walker) = uia.ControlViewWalker() {
            let mut current = focused.clone();
            for _ in 0..4 {
                match walker.GetParentElement(&current) {
                    Ok(parent) => {
                        if let Some(pt) = try_uia_element_caret(&parent) {
                            return Some(pt);
                        }
                        current = parent;
                    }
                    Err(_) => break,
                }
            }
        }

        // Last resort: use focused element's bounding rectangle
        // This works for Explorer rename boxes and address bar
        try_bounding_rect_fallback(&focused)
    }
}

fn try_uia_element_caret(element: &IUIAutomationElement) -> Option<ScreenPoint> {
    unsafe {
        // Try TextPattern2.GetCaretRange first (Explorer, modern XAML controls)
        if let Ok(pat2) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
        {
            let mut is_active = BOOL(0);
            if let Ok(range) = pat2.GetCaretRange(&mut is_active)
                && let Some(pt) = point_from_range(&range)
            {
                return Some(pt);
            }
        }

        // Fallback: TextPattern.GetSelection (Office, Edge)
        if let Ok(pat) = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            && let Ok(ranges) = pat.GetSelection()
        {
            let len = ranges.Length().unwrap_or(0);
            if len > 0
                && let Ok(range) = ranges.GetElement(0)
                && let Some(pt) = point_from_range(&range)
            {
                return Some(pt);
            }
        }

        None
    }
}

/// Fallback: use the focused element's bounding rectangle.
/// Works for Explorer rename boxes and other simple input controls.
fn try_bounding_rect_fallback(element: &IUIAutomationElement) -> Option<ScreenPoint> {
    unsafe {
        let rect = element.CurrentBoundingRectangle().ok()?;

        let x = rect.left as f64;
        let y = rect.top as f64;
        let w = (rect.right - rect.left) as f64;
        let h = (rect.bottom - rect.top) as f64;

        if w <= 0.0 || h <= 0.0 {
            return None;
        }

        // Return center-x, bottom-y of the element
        Some(ScreenPoint {
            x: (x + w / 2.0) as i32,
            y: (y + h) as i32,
            caret_top: y as i32,
        })
    }
}

fn point_from_range(range: &IUIAutomationTextRange) -> Option<ScreenPoint> {
    unsafe {
        let rects_sa = range.GetBoundingRectangles().ok()?;
        let rects = owned_safearray_to_f64s(rects_sa)?;

        if rects.len() >= 4 && rects[3] > 0.0 {
            return Some(ScreenPoint {
                x: rects[0] as i32,
                y: (rects[1] + rects[3]) as i32,
                caret_top: rects[1] as i32,
            });
        }

        // Degenerate range: expand to character
        let _ = range.ExpandToEnclosingUnit(TextUnit_Character);
        let rects_sa = range.GetBoundingRectangles().ok()?;
        let rects = owned_safearray_to_f64s(rects_sa)?;

        if rects.len() >= 4 && rects[3] > 0.0 {
            return Some(ScreenPoint {
                x: rects[0] as i32,
                y: (rects[1] + rects[3]) as i32,
                caret_top: rects[1] as i32,
            });
        }

        None
    }
}

struct OwnedSafeArray(*mut SAFEARRAY);

impl OwnedSafeArray {
    /// Takes ownership of a SAFEARRAY returned by a COM method.
    unsafe fn from_raw(value: *mut SAFEARRAY) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }
}

impl Drop for OwnedSafeArray {
    fn drop(&mut self) {
        unsafe {
            let _ = SafeArrayDestroy(self.0);
        }
    }
}

fn safe_array_value_count(lower: i32, upper: i32) -> Option<usize> {
    if upper < lower {
        return None;
    }
    let count = upper.checked_sub(lower)?.checked_add(1)?;
    usize::try_from(count)
        .ok()
        .filter(|count| *count <= MAX_BOUNDING_RECT_VALUES)
}

/// Copies doubles from an owned SAFEARRAY and releases it on every return path.
unsafe fn owned_safearray_to_f64s(raw: *mut SAFEARRAY) -> Option<Vec<f64>> {
    unsafe {
        let safe_array = OwnedSafeArray::from_raw(raw)?;
        let lower = SafeArrayGetLBound(safe_array.0, 1).ok()?;
        let upper = SafeArrayGetUBound(safe_array.0, 1).ok()?;
        let count = safe_array_value_count(lower, upper)?;
        let mut result = Vec::with_capacity(count);
        for offset in 0..count {
            let index = lower.checked_add(i32::try_from(offset).ok()?)?;
            let mut val = 0.0f64;
            SafeArrayGetElement(
                safe_array.0,
                &index,
                (&raw mut val).cast::<std::ffi::c_void>(),
            )
            .ok()?;
            result.push(val);
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_array_bounds_are_checked_before_allocation() {
        assert_eq!(safe_array_value_count(0, 3), Some(4));
        assert_eq!(safe_array_value_count(5, 4), None);
        assert_eq!(safe_array_value_count(i32::MIN, i32::MAX), None);
        assert_eq!(
            safe_array_value_count(0, MAX_BOUNDING_RECT_VALUES as i32),
            None
        );
    }
}
