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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretQuality {
    /// A text API returned a usable caret/range rectangle, or Win32 returned
    /// a caret with a normal text height.
    Reliable,
    /// A collapsed UIA range exposed a neighboring character, not the caret
    /// itself. Use its edge as a typing-position estimate, but do not cache it.
    TextNeighbor,
    /// No caret was exposed, so the focused element bounds are the best
    /// available anchor. Keep the existing below-the-element placement.
    ElementFallback,
    /// A compact edit control exposed bounds but no usable text rectangle.
    EditFallback,
    /// The X coordinate is useful, but the reported caret height/Y is not.
    Unreliable,
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,         // caret bottom (for placing bubble below)
    pub caret_top: i32, // caret top (for placing bubble above)
    pub quality: CaretQuality,
    pub element_rect: Option<ScreenRect>,
}

impl Default for ScreenPoint {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            caret_top: 0,
            quality: CaretQuality::Reliable,
            element_rect: None,
        }
    }
}

impl ScreenPoint {
    fn from_top_bottom(
        x: i32,
        top: i32,
        bottom: i32,
        quality: CaretQuality,
        element_rect: Option<ScreenRect>,
    ) -> Self {
        Self {
            x,
            y: bottom,
            caret_top: top,
            quality,
            element_rect,
        }
    }

    fn is_reliable(self) -> bool {
        self.quality == CaretQuality::Reliable
    }
}

pub fn get_caret_screen_position() -> Option<ScreenPoint> {
    let gui = try_gui_thread_info();
    if gui.is_some_and(ScreenPoint::is_reliable) {
        return gui;
    }

    let msaa = try_msaa_caret();
    if msaa.is_some_and(ScreenPoint::is_reliable) {
        return msaa;
    }

    select_caret_candidate(gui, msaa, try_uia_caret())
}

fn select_caret_candidate(
    gui: Option<ScreenPoint>,
    msaa: Option<ScreenPoint>,
    uia: UiaCaretProbe,
) -> Option<ScreenPoint> {
    // Strategy 1: Win32 GetGUIThreadInfo (Notepad, classic Win32 apps)
    if gui.is_some_and(ScreenPoint::is_reliable) {
        return gui;
    }

    // Strategy 2: MSAA IAccessible OBJID_CARET (Chrome, many apps)
    if msaa.is_some_and(ScreenPoint::is_reliable) {
        return msaa;
    }

    // Strategy 3+4: COM UI Automation (Explorer, modern controls, Office, Edge)
    if let Some(pt) = uia.point
        && (pt.is_reliable()
            || pt.quality == CaretQuality::TextNeighbor
            || (gui.is_none() && msaa.is_none()))
    {
        return Some(pt);
    }

    // A tiny Win32/MSAA caret still tracks the insertion point horizontally.
    // Keep that X coordinate, but let layout derive a safer Y from UIA bounds
    // or an estimated line height.
    if let Some(mut pt) = gui {
        pt.element_rect = pt.element_rect.or(uia.element_rect);
        return Some(pt);
    }
    if let Some(mut pt) = msaa {
        pt.element_rect = pt.element_rect.or(uia.element_rect);
        return Some(pt);
    }

    uia.point
}

#[derive(Debug, Clone, Copy, Default)]
struct UiaCaretProbe {
    point: Option<ScreenPoint>,
    element_rect: Option<ScreenRect>,
}

#[derive(Debug, Clone, Copy, Default)]
struct UiaElementCaret {
    point: Option<ScreenPoint>,
    has_text_pattern: bool,
}

fn is_tall_caret(height: i32, dpi_scale: f32) -> bool {
    height as f32 >= 8.0_f32.max(8.0 * dpi_scale)
}

fn window_dpi_scale(hwnd: HWND) -> f32 {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        if dpi > 0 { dpi as f32 / 96.0 } else { 1.0 }
    }
}

fn screen_rect_from_rect(rect: RECT) -> Option<ScreenRect> {
    (rect.right > rect.left && rect.bottom > rect.top).then_some(ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn is_compact_edit_rect(
    control_type: UIA_CONTROLTYPE_ID,
    rect: ScreenRect,
    dpi_scale: f32,
) -> bool {
    control_type == UIA_EditControlTypeId && rect.bottom - rect.top <= (240.0 * dpi_scale) as i32
}

fn is_multiline_edit_candidate(rect: ScreenRect, dpi_scale: f32) -> bool {
    rect.bottom - rect.top >= (80.0 * dpi_scale) as i32
}

fn nearby_edit_container(
    edit: ScreenRect,
    parent: ScreenRect,
    dpi_scale: f32,
) -> Option<ScreenRect> {
    let padding = (64.0 * dpi_scale) as i32;
    (parent.left <= edit.left
        && edit.right <= parent.right
        && parent.top <= edit.top
        && edit.bottom <= parent.bottom
        && edit.left - parent.left <= padding
        && parent.right - edit.right <= padding
        && edit.top - parent.top <= padding
        && parent.bottom - edit.bottom <= padding
        && parent.bottom - parent.top <= (240.0 * dpi_scale) as i32)
        .then_some(parent)
}

fn point_from_edit_rect(rect: ScreenRect) -> ScreenPoint {
    ScreenPoint::from_top_bottom(
        rect.left + (rect.right - rect.left) / 2,
        rect.top,
        rect.bottom,
        CaretQuality::EditFallback,
        Some(rect),
    )
}

fn anchor_uia_point_in_edit(
    mut point: ScreenPoint,
    rect: ScreenRect,
    dpi_scale: f32,
) -> ScreenPoint {
    // Selection ranges in short Qt edits can report a text rectangle whose
    // bottom is still inside the input field. Keep the text X, but place the
    // popup below the field. Tall multiline composers keep their text Y.
    if point.quality == CaretQuality::Reliable && !is_multiline_edit_candidate(rect, dpi_scale) {
        point.caret_top = rect.top;
        point.y = rect.bottom;
    }
    point.element_rect = Some(rect);
    point
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

fn try_uia_caret() -> UiaCaretProbe {
    // Wrap in catch_unwind because COM UIA calls can sometimes fail unexpectedly
    std::panic::catch_unwind(try_uia_caret_inner)
        .ok()
        .unwrap_or_default()
}

fn try_uia_caret_inner() -> UiaCaretProbe {
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 as _,
        ));

        let uia: IUIAutomation = match CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
        {
            Ok(uia) => uia,
            Err(_) => return UiaCaretProbe::default(),
        };
        let Ok(focused) = uia.GetFocusedElement() else {
            return UiaCaretProbe::default();
        };
        let raw_focused_rect = current_element_rect(&focused);
        let control_type = focused.CurrentControlType().ok();
        let dpi_scale = window_dpi_scale(GetForegroundWindow());
        let mut focused_rect = raw_focused_rect.filter(|rect| {
            control_type.is_some_and(|kind| is_compact_edit_rect(kind, *rect, dpi_scale))
        });
        let edit_rect = focused_rect;
        let mut has_text_pattern = false;

        // Qt may expose only the text area as the focused Edit. Include a
        // nearby composer container before deciding whether this is a
        // multiline editor; a phone-number field must not use text-neighbor
        // geometry, which can overlap the digits.
        if let Some(edit_rect) = edit_rect
            && let Ok(walker) = uia.ControlViewWalker()
            && let Ok(parent) = walker.GetParentElement(&focused)
            && let Some(parent_rect) = current_element_rect(&parent)
        {
            focused_rect =
                nearby_edit_container(edit_rect, parent_rect, dpi_scale).or(focused_rect);
        }

        // Try focused element directly
        let allow_neighbor =
            focused_rect.is_some_and(|rect| is_multiline_edit_candidate(rect, dpi_scale));
        let focused_caret = try_uia_element_caret(&focused, allow_neighbor);
        has_text_pattern |= focused_caret.has_text_pattern;
        if let Some(mut pt) = focused_caret.point {
            if let Some(rect) = focused_rect {
                pt = anchor_uia_point_in_edit(pt, rect, dpi_scale);
            }
            return UiaCaretProbe {
                point: Some(pt),
                element_rect: focused_rect,
            };
        }

        // Walk up the tree (max 4 levels)
        if let Ok(walker) = uia.ControlViewWalker() {
            let mut current = focused.clone();
            for _ in 0..4 {
                match walker.GetParentElement(&current) {
                    Ok(parent) => {
                        if let Some(edit_rect) = edit_rect
                            && let Some(parent_rect) = current_element_rect(&parent)
                        {
                            focused_rect = nearby_edit_container(edit_rect, parent_rect, dpi_scale)
                                .or(focused_rect);
                        }
                        let parent_caret = try_uia_element_caret(&parent, false);
                        has_text_pattern |= parent_caret.has_text_pattern;
                        if let Some(mut pt) = parent_caret.point {
                            if let Some(rect) = focused_rect {
                                pt = anchor_uia_point_in_edit(pt, rect, dpi_scale);
                            }
                            return UiaCaretProbe {
                                point: Some(pt),
                                element_rect: focused_rect,
                            };
                        }
                        current = parent;
                    }
                    Err(_) => break,
                }
            }
        }

        // A compact edit control is a useful anchor even when its text
        // pattern exposes no caret. Large text/document surfaces are not.
        if !has_text_pattern {
            return UiaCaretProbe {
                point: try_bounding_rect_fallback(&focused),
                element_rect: raw_focused_rect,
            };
        }
        if focused_rect.is_some() {
            return UiaCaretProbe {
                point: focused_rect.map(point_from_edit_rect),
                element_rect: focused_rect,
            };
        }

        // A text pattern exists but did not expose a rectangle. Do not turn
        // the entire text control into a fake caret; retain its bounds only
        // so a low-level tiny caret can use the element bottom for Y.
        UiaCaretProbe {
            point: None,
            element_rect: focused_rect,
        }
    }
}

fn current_element_rect(element: &IUIAutomationElement) -> Option<ScreenRect> {
    unsafe { screen_rect_from_rect(element.CurrentBoundingRectangle().ok()?) }
}

fn try_uia_element_caret(element: &IUIAutomationElement, allow_neighbor: bool) -> UiaElementCaret {
    unsafe {
        let mut has_text_pattern = false;
        let mut neighbor = None;

        // Try TextPattern2.GetCaretRange first (Explorer, modern XAML controls)
        if let Ok(pat2) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
        {
            has_text_pattern = true;
            let mut is_active = BOOL(0);
            if let Ok(range) = pat2.GetCaretRange(&mut is_active)
                && let Some(pt) = point_from_range(&range, allow_neighbor)
            {
                if pt.quality == CaretQuality::Reliable {
                    return UiaElementCaret {
                        point: Some(pt),
                        has_text_pattern,
                    };
                }
                neighbor = Some(pt);
            }
        }

        // Fallback: TextPattern.GetSelection (Office, Edge)
        if let Ok(pat) = element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        {
            has_text_pattern = true;
            if let Ok(ranges) = pat.GetSelection() {
                let len = ranges.Length().unwrap_or(0);
                if len > 0
                    && let Ok(range) = ranges.GetElement(0)
                    && let Some(pt) = point_from_range(&range, allow_neighbor)
                {
                    if pt.quality == CaretQuality::Reliable {
                        return UiaElementCaret {
                            point: Some(pt),
                            has_text_pattern,
                        };
                    }
                    neighbor = neighbor.or(Some(pt));
                }
            }
        }

        UiaElementCaret {
            point: neighbor,
            has_text_pattern,
        }
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
        let rect = ScreenRect {
            left: x as i32,
            top: y as i32,
            right: (x + w) as i32,
            bottom: (y + h) as i32,
        };

        Some(ScreenPoint::from_top_bottom(
            (x + w / 2.0) as i32,
            rect.top,
            rect.bottom,
            CaretQuality::ElementFallback,
            Some(rect),
        ))
    }
}

fn range_rectangles(range: &IUIAutomationTextRange) -> Option<Vec<f64>> {
    unsafe {
        let rects_sa = range.GetBoundingRectangles().ok()?;
        owned_safearray_to_f64s(rects_sa)
    }
}

fn point_from_rectangles(
    rects: &[f64],
    right_edge: bool,
    quality: CaretQuality,
) -> Option<ScreenPoint> {
    let [left, top, width, height, ..] = rects else {
        return None;
    };
    if !left.is_finite()
        || !top.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || *width < 0.0
        || *height <= 0.0
        || (right_edge && *width == 0.0)
    {
        return None;
    }
    Some(ScreenPoint::from_top_bottom(
        (left + if right_edge { *width } else { 0.0 }) as i32,
        *top as i32,
        (top + height) as i32,
        quality,
        None,
    ))
}

fn point_from_range(range: &IUIAutomationTextRange, allow_neighbor: bool) -> Option<ScreenPoint> {
    unsafe {
        if let Some(pt) = range_rectangles(range)
            .and_then(|rects| point_from_rectangles(&rects, false, CaretQuality::Reliable))
        {
            return Some(pt);
        }

        let collapsed = range
            .CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                range,
                TextPatternRangeEndpoint_End,
            )
            .ok()
            == Some(0);
        if collapsed && !allow_neighbor {
            return None;
        }

        // Work on clones: the selection may still supply a better rectangle.
        if let Ok(expanded) = range.Clone() {
            let _ = expanded.ExpandToEnclosingUnit(TextUnit_Character);
            if let Some(pt) = range_rectangles(&expanded).and_then(|rects| {
                point_from_rectangles(
                    &rects,
                    false,
                    if collapsed {
                        CaretQuality::TextNeighbor
                    } else {
                        CaretQuality::Reliable
                    },
                )
            }) {
                return Some(pt);
            }
        }

        if !allow_neighbor || !collapsed {
            return None;
        }
        let neighbor = range.Clone().ok()?;
        if neighbor
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -1)
            .ok()?
            != -1
        {
            return None;
        }
        range_rectangles(&neighbor)
            .and_then(|rects| point_from_rectangles(&rects, true, CaretQuality::TextNeighbor))
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

    fn point(x: i32, height: i32, quality: CaretQuality) -> ScreenPoint {
        ScreenPoint::from_top_bottom(x, 100, 100 + height, quality, None)
    }

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

    #[test]
    fn collapsed_range_neighbor_uses_character_edge_without_changing_direct_range() {
        let rect = [100.0, 200.0, 12.0, 20.0];
        let direct = point_from_rectangles(&rect, false, CaretQuality::Reliable).unwrap();
        let neighbor = point_from_rectangles(&rect, true, CaretQuality::TextNeighbor).unwrap();
        assert_eq!((direct.x, direct.y), (100, 220));
        assert_eq!((neighbor.x, neighbor.y), (112, 220));
        assert!(!neighbor.is_reliable());
        assert!(
            point_from_rectangles(&[100.0, 200.0, 0.0, 20.0], false, CaretQuality::Reliable)
                .is_some()
        );
        assert!(
            point_from_rectangles(&[100.0, 200.0, 0.0, 20.0], true, CaretQuality::TextNeighbor)
                .is_none()
        );
    }

    #[test]
    fn text_neighbor_replaces_tiny_caret_but_not_reliable_gui() {
        let neighbor = point(400, 20, CaretQuality::TextNeighbor);
        let uia = UiaCaretProbe {
            point: Some(neighbor),
            element_rect: None,
        };
        assert_eq!(
            select_caret_candidate(Some(point(100, 2, CaretQuality::Unreliable)), None, uia)
                .unwrap()
                .x,
            400
        );
        assert_eq!(
            select_caret_candidate(Some(point(100, 20, CaretQuality::Reliable)), None, uia)
                .unwrap()
                .x,
            100
        );
    }

    #[test]
    fn caret_height_threshold_keeps_normal_carets_reliable() {
        assert!(!is_tall_caret(2, 1.5));
        assert!(!is_tall_caret(11, 1.5));
        assert!(is_tall_caret(12, 1.5));
        assert!(is_tall_caret(17, 1.5));
        assert!(is_tall_caret(24, 1.5));
    }

    #[test]
    fn only_compact_edit_bounds_can_replace_an_empty_text_pattern() {
        let field = ScreenRect {
            left: 100,
            top: 100,
            right: 600,
            bottom: 214,
        };
        assert!(is_compact_edit_rect(UIA_EditControlTypeId, field, 1.0));
        assert!(!is_compact_edit_rect(UIA_TextControlTypeId, field, 1.0));
        assert!(!is_compact_edit_rect(
            UIA_EditControlTypeId,
            ScreenRect {
                bottom: 500,
                ..field
            },
            1.0,
        ));
    }

    #[test]
    fn neighbor_caret_is_limited_to_multiline_sized_editors() {
        let phone = ScreenRect {
            left: 430,
            top: 503,
            right: 879,
            bottom: 555,
        };
        let composer = ScreenRect {
            left: 23,
            top: 747,
            right: 584,
            bottom: 929,
        };
        assert!(!is_multiline_edit_candidate(phone, 1.0));
        assert!(!is_multiline_edit_candidate(phone, 1.5));
        assert!(is_multiline_edit_candidate(composer, 1.0));
        assert!(is_multiline_edit_candidate(composer, 1.5));
    }

    #[test]
    fn short_edit_selection_uses_field_bottom_without_losing_text_x() {
        let phone = ScreenRect {
            left: 290,
            top: 225,
            right: 651,
            bottom: 277,
        };
        let text_point = ScreenPoint::from_top_bottom(370, 226, 259, CaretQuality::Reliable, None);
        let anchored = anchor_uia_point_in_edit(text_point, phone, 1.0);
        assert_eq!(
            (anchored.x, anchored.y, anchored.caret_top),
            (370, 277, 225)
        );
        assert_eq!(anchored.element_rect, Some(phone));

        let composer = ScreenRect {
            left: 23,
            top: 747,
            right: 584,
            bottom: 929,
        };
        let multiline = anchor_uia_point_in_edit(text_point, composer, 1.0);
        assert_eq!((multiline.x, multiline.y), (370, 259));
    }

    #[test]
    fn nearby_edit_container_covers_composer_chrome_but_not_a_large_panel() {
        let edit = ScreenRect {
            left: 180,
            top: 515,
            right: 680,
            bottom: 640,
        };
        let composer = ScreenRect {
            left: 158,
            top: 510,
            right: 720,
            bottom: 696,
        };
        assert_eq!(nearby_edit_container(edit, composer, 1.0), Some(composer));
        assert_eq!(
            nearby_edit_container(
                edit,
                ScreenRect {
                    bottom: 858,
                    ..composer
                },
                1.0,
            ),
            None,
        );
    }

    #[test]
    fn reliable_gui_result_keeps_existing_strategy_priority() {
        let selected = select_caret_candidate(
            Some(point(10, 24, CaretQuality::Reliable)),
            Some(point(20, 30, CaretQuality::Reliable)),
            UiaCaretProbe {
                point: Some(point(30, 28, CaretQuality::Reliable)),
                element_rect: None,
            },
        )
        .expect("a caret candidate");

        assert_eq!(selected.x, 10);
        assert_eq!(selected.quality, CaretQuality::Reliable);
    }

    #[test]
    fn tall_msaa_result_replaces_only_a_tiny_gui_result() {
        let selected = select_caret_candidate(
            Some(point(10, 2, CaretQuality::Unreliable)),
            Some(point(20, 30, CaretQuality::Reliable)),
            UiaCaretProbe::default(),
        )
        .expect("a caret candidate");

        assert_eq!(selected.x, 20);
        assert_eq!(selected.quality, CaretQuality::Reliable);
    }

    #[test]
    fn tiny_gui_result_keeps_x_and_adopts_uia_element_bounds() {
        let element_rect = ScreenRect {
            left: 100,
            top: 200,
            right: 500,
            bottom: 260,
        };
        let selected = select_caret_candidate(
            Some(point(350, 2, CaretQuality::Unreliable)),
            None,
            UiaCaretProbe {
                point: None,
                element_rect: Some(element_rect),
            },
        )
        .expect("a caret candidate");

        assert_eq!(selected.x, 350);
        assert_eq!(selected.quality, CaretQuality::Unreliable);
        assert_eq!(selected.element_rect, Some(element_rect));
    }

    #[test]
    fn patternless_uia_element_fallback_is_preserved_without_low_level_caret() {
        let fallback = point(300, 60, CaretQuality::ElementFallback);
        let selected = select_caret_candidate(
            None,
            None,
            UiaCaretProbe {
                point: Some(fallback),
                element_rect: None,
            },
        )
        .expect("a caret candidate");

        assert_eq!(selected.x, 300);
        assert_eq!(selected.quality, CaretQuality::ElementFallback);
    }
}
