//! UI Automation traversal, text-range geometry, and focused-field fallbacks.
mod safearray;
mod text_range;

use super::native::window_dpi_scale;
use super::{CaretQuality, ScreenPoint, ScreenRect};
use text_range::point_from_range;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::BOOL;

const MIN_MULTILINE_EDIT_HEIGHT_DIP: f32 = 80.0;
const MAX_COMPACT_EDIT_HEIGHT_DIP: f32 = 240.0;
const MAX_EDIT_CONTAINER_PADDING_DIP: f32 = 64.0;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct UiaCaretProbe {
    pub(super) point: Option<ScreenPoint>,
    pub(super) element_rect: Option<ScreenRect>,
}

#[derive(Debug, Clone, Copy, Default)]
struct UiaElementCaret {
    point: Option<ScreenPoint>,
    has_text_pattern: bool,
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
    control_type == UIA_EditControlTypeId
        && rect.height() <= (MAX_COMPACT_EDIT_HEIGHT_DIP * dpi_scale) as i32
}

fn is_multiline_edit_candidate(rect: ScreenRect, dpi_scale: f32) -> bool {
    rect.height() >= (MIN_MULTILINE_EDIT_HEIGHT_DIP * dpi_scale) as i32
}

fn nearby_edit_container(
    edit: ScreenRect,
    parent: ScreenRect,
    dpi_scale: f32,
) -> Option<ScreenRect> {
    let padding = (MAX_EDIT_CONTAINER_PADDING_DIP * dpi_scale) as i32;
    (parent.contains(edit)
        && edit.left - parent.left <= padding
        && parent.right - edit.right <= padding
        && edit.top - parent.top <= padding
        && parent.bottom - edit.bottom <= padding
        && parent.height() <= (MAX_COMPACT_EDIT_HEIGHT_DIP * dpi_scale) as i32)
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
pub(super) fn try_uia_caret() -> UiaCaretProbe {
    // Wrap in catch_unwind because COM UIA calls can sometimes fail unexpectedly
    std::panic::catch_unwind(try_uia_caret_inner)
        .ok()
        .unwrap_or_default()
}

fn try_uia_caret_inner() -> UiaCaretProbe {
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let uia: IUIAutomation = match CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
        {
            Ok(uia) => uia,
            Err(_) => return UiaCaretProbe::default(),
        };
        let Ok(focused) = uia.GetFocusedElement() else {
            return UiaCaretProbe::default();
        };
        let focused_bounds = current_element_rect(&focused);
        let control_type = focused.CurrentControlType().ok();
        let dpi_scale = window_dpi_scale(GetForegroundWindow());
        let focused_edit_bounds = focused_bounds.filter(|rect| {
            control_type.is_some_and(|kind| is_compact_edit_rect(kind, *rect, dpi_scale))
        });
        let mut placement_bounds = focused_edit_bounds;
        // Qt may expose only the text area as the focused Edit. Include a
        // nearby composer container before deciding whether this is a
        // multiline editor; a phone-number field must not use text-neighbor
        // geometry, which can overlap the digits.
        let walker = focused_edit_bounds.and_then(|_| uia.ControlViewWalker().ok());
        let first_parent = walker
            .as_ref()
            .and_then(|walker| walker.GetParentElement(&focused).ok());
        let first_parent_rect = first_parent.as_ref().and_then(current_element_rect);
        if let Some(edit_bounds) = focused_edit_bounds {
            placement_bounds = first_parent_rect
                .and_then(|parent| nearby_edit_container(edit_bounds, parent, dpi_scale))
                .or(placement_bounds);
        }

        // Try focused element directly
        let allow_neighbor =
            placement_bounds.is_some_and(|rect| is_multiline_edit_candidate(rect, dpi_scale));
        let focused_caret =
            probe_uia_element(&focused, placement_bounds, dpi_scale, allow_neighbor);
        if let Some(pt) = focused_caret.point {
            return UiaCaretProbe {
                point: Some(pt),
                element_rect: placement_bounds,
            };
        }

        // Walk up the tree (max 4 levels)
        if let Some(walker) = walker.or_else(|| uia.ControlViewWalker().ok()) {
            let mut parent = first_parent.or_else(|| walker.GetParentElement(&focused).ok());
            for depth in 0..4 {
                let Some(current) = parent else { break };
                if depth > 0
                    && let Some(edit_bounds) = focused_edit_bounds
                {
                    placement_bounds = current_element_rect(&current)
                        .and_then(|parent| nearby_edit_container(edit_bounds, parent, dpi_scale))
                        .or(placement_bounds);
                }
                if let Some(pt) =
                    probe_uia_element(&current, placement_bounds, dpi_scale, false).point
                {
                    return UiaCaretProbe {
                        point: Some(pt),
                        element_rect: placement_bounds,
                    };
                }
                parent = walker.GetParentElement(&current).ok();
            }
        }

        finish_uia_probe(
            focused_bounds,
            placement_bounds,
            focused_caret.has_text_pattern,
        )
    }
}

fn current_element_rect(element: &IUIAutomationElement) -> Option<ScreenRect> {
    unsafe { screen_rect_from_rect(element.CurrentBoundingRectangle().ok()?) }
}

fn probe_uia_element(
    element: &IUIAutomationElement,
    edit_rect: Option<ScreenRect>,
    dpi_scale: f32,
    allow_neighbor: bool,
) -> UiaElementCaret {
    let mut probe = try_uia_element_caret(element, allow_neighbor);
    probe.point = probe.point.map(|point| {
        edit_rect.map_or(point, |rect| {
            anchor_uia_point_in_edit(point, rect, dpi_scale)
        })
    });
    probe
}

fn finish_uia_probe(
    focused_bounds: Option<ScreenRect>,
    placement_bounds: Option<ScreenRect>,
    focused_has_text_pattern: bool,
) -> UiaCaretProbe {
    if !focused_has_text_pattern {
        return UiaCaretProbe {
            point: focused_bounds.map(point_from_element_rect),
            element_rect: focused_bounds,
        };
    }

    // An empty text pattern must not turn a large document into a fake caret.
    UiaCaretProbe {
        point: placement_bounds.map(point_from_edit_rect),
        element_rect: placement_bounds,
    }
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

/// Fallback for focused controls without a text pattern (e.g. Explorer rename).
fn point_from_element_rect(rect: ScreenRect) -> ScreenPoint {
    ScreenPoint::from_top_bottom(
        ((rect.left as f64 + rect.right as f64) / 2.0) as i32,
        rect.top,
        rect.bottom,
        CaretQuality::ElementFallback,
        Some(rect),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parent_text_pattern_does_not_suppress_focused_element_fallback() {
        let focused = ScreenRect {
            left: 100,
            top: 200,
            right: 300,
            bottom: 240,
        };
        // Ancestors may expose empty text patterns; only the focused element's
        // pattern status decides whether its bounds are a valid fallback.
        let probe = finish_uia_probe(Some(focused), None, false);
        let point = probe.point.expect("focused element bounds fallback");
        assert_eq!(point.quality, CaretQuality::ElementFallback);
        assert_eq!((point.x, point.y), (200, 240));
        assert_eq!(probe.element_rect, Some(focused));
    }

    #[test]
    fn empty_focused_text_pattern_uses_only_compact_edit_bounds() {
        let edit = ScreenRect {
            left: 100,
            top: 200,
            right: 300,
            bottom: 240,
        };
        let compact = finish_uia_probe(Some(edit), Some(edit), true);
        assert_eq!(compact.point.unwrap().quality, CaretQuality::EditFallback);
        let document = finish_uia_probe(Some(edit), None, true);
        assert!(document.point.is_none());
        assert!(document.element_rect.is_none());
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
}
