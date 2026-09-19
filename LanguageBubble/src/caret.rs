//! Caret geometry and provider priority. Platform probes live in child modules.
mod native;
mod uia;

use native::{try_gui_thread_info, try_msaa_caret};
use uia::{UiaCaretProbe, try_uia_caret};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    fn height(self) -> i32 {
        self.bottom - self.top
    }

    fn contains(self, other: Self) -> bool {
        self.left <= other.left
            && other.right <= self.right
            && self.top <= other.top
            && other.bottom <= self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretQuality {
    /// A text API returned a usable caret/range rectangle, or Win32 returned
    /// a caret with a normal text height.
    Reliable,
    /// A collapsed UIA range exposed a neighboring character, not the caret
    /// itself. Use its edge as a typing-position estimate.
    TextNeighbor,
    /// No caret was exposed, so the focused element bounds are the best
    /// available anchor. Keep the existing below-the-element placement.
    ElementFallback,
    /// A compact edit control exposed bounds but no usable text rectangle.
    EditFallback,
    /// The X coordinate is useful, but the reported caret height/Y is not.
    Unreliable,
}

impl CaretQuality {
    pub(crate) fn is_field_fallback(self) -> bool {
        matches!(self, Self::EditFallback | Self::Unreliable)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,         // caret bottom (for placing bubble below)
    pub caret_top: i32, // caret top (for placing bubble above)
    pub quality: CaretQuality,
    pub element_rect: Option<ScreenRect>,
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
    select_caret_candidate(try_gui_thread_info(), try_msaa_caret, try_uia_caret)
}

fn select_caret_candidate(
    gui: Option<ScreenPoint>,
    msaa: impl FnOnce() -> Option<ScreenPoint>,
    uia: impl FnOnce() -> UiaCaretProbe,
) -> Option<ScreenPoint> {
    // Strategy 1: Win32 GetGUIThreadInfo (Notepad, classic Win32 apps)
    if gui.is_some_and(ScreenPoint::is_reliable) {
        return gui;
    }

    // Strategy 2: MSAA IAccessible OBJID_CARET (Chrome, many apps)
    let msaa = msaa();
    if msaa.is_some_and(ScreenPoint::is_reliable) {
        return msaa;
    }

    // Strategy 3+4: COM UI Automation (Explorer, modern controls, Office, Edge)
    let uia = uia();
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
    if let Some(mut pt) = gui.or(msaa) {
        pt.element_rect = pt.element_rect.or(uia.element_rect);
        return Some(pt);
    }

    uia.point
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::Cell;

    fn point(x: i32, height: i32, quality: CaretQuality) -> ScreenPoint {
        ScreenPoint::from_top_bottom(x, 100, 100 + height, quality, None)
    }

    #[test]
    fn text_neighbor_replaces_tiny_caret_but_not_reliable_gui() {
        let neighbor = point(400, 20, CaretQuality::TextNeighbor);
        let uia = UiaCaretProbe {
            point: Some(neighbor),
            element_rect: None,
        };
        assert_eq!(
            select_caret_candidate(
                Some(point(100, 2, CaretQuality::Unreliable)),
                || None,
                || uia
            )
            .unwrap()
            .x,
            400
        );
        assert_eq!(
            select_caret_candidate(
                Some(point(100, 20, CaretQuality::Reliable)),
                || None,
                || uia
            )
            .unwrap()
            .x,
            100
        );
    }

    #[test]
    fn reliable_gui_result_keeps_existing_strategy_priority() {
        let selected = select_caret_candidate(
            Some(point(10, 24, CaretQuality::Reliable)),
            || Some(point(20, 30, CaretQuality::Reliable)),
            || UiaCaretProbe {
                point: Some(point(30, 28, CaretQuality::Reliable)),
                element_rect: None,
            },
        )
        .expect("a caret candidate");

        assert_eq!(selected.x, 10);
        assert_eq!(selected.quality, CaretQuality::Reliable);
    }

    #[test]
    fn reliable_provider_skips_slower_probes() {
        let msaa_called = Cell::new(false);
        let uia_called = Cell::new(false);
        let selected = select_caret_candidate(
            Some(point(10, 24, CaretQuality::Reliable)),
            || {
                msaa_called.set(true);
                None
            },
            || {
                uia_called.set(true);
                UiaCaretProbe::default()
            },
        );
        assert_eq!(selected.unwrap().x, 10);
        assert!(!msaa_called.get());
        assert!(!uia_called.get());
    }

    #[test]
    fn reliable_msaa_skips_uia() {
        let uia_called = Cell::new(false);
        let selected = select_caret_candidate(
            Some(point(10, 2, CaretQuality::Unreliable)),
            || Some(point(20, 24, CaretQuality::Reliable)),
            || {
                uia_called.set(true);
                UiaCaretProbe::default()
            },
        );
        assert_eq!(selected.unwrap().x, 20);
        assert!(!uia_called.get());
    }

    #[test]
    fn missing_current_providers_do_not_reuse_an_old_point() {
        assert!(select_caret_candidate(None, || None, UiaCaretProbe::default).is_none());
    }

    #[test]
    fn tall_msaa_result_replaces_only_a_tiny_gui_result() {
        let selected = select_caret_candidate(
            Some(point(10, 2, CaretQuality::Unreliable)),
            || Some(point(20, 30, CaretQuality::Reliable)),
            UiaCaretProbe::default,
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
            || None,
            || UiaCaretProbe {
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
            || None,
            || UiaCaretProbe {
                point: Some(fallback),
                element_rect: None,
            },
        )
        .expect("a caret candidate");

        assert_eq!(selected.x, 300);
        assert_eq!(selected.quality, CaretQuality::ElementFallback);
    }
}
