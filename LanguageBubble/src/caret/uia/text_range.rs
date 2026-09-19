//! Convert UI Automation text ranges into direct or neighboring caret anchors.
use super::super::{CaretQuality, ScreenPoint};
use super::safearray::owned_safearray_to_f64s;
use windows::Win32::UI::Accessibility::*;

fn range_rectangles(range: &IUIAutomationTextRange) -> Option<Vec<f64>> {
    unsafe {
        let rects_sa = range.GetBoundingRectangles().ok()?;
        owned_safearray_to_f64s(rects_sa)
    }
}

#[derive(Clone, Copy)]
enum RectangleEdge {
    Left,
    Right,
}

fn point_from_rectangles(
    rects: &[f64],
    edge: RectangleEdge,
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
    {
        return None;
    }
    let x = match edge {
        RectangleEdge::Left => *left,
        RectangleEdge::Right if *width > 0.0 => left + width,
        RectangleEdge::Right => return None,
    };
    Some(ScreenPoint::from_top_bottom(
        x as i32,
        *top as i32,
        (top + height) as i32,
        quality,
        None,
    ))
}

pub(super) fn point_from_range(
    range: &IUIAutomationTextRange,
    allow_neighbor: bool,
) -> Option<ScreenPoint> {
    unsafe {
        if let Some(pt) = range_rectangles(range).and_then(|rects| {
            point_from_rectangles(&rects, RectangleEdge::Left, CaretQuality::Reliable)
        }) {
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
                    RectangleEdge::Left,
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
        range_rectangles(&neighbor).and_then(|rects| {
            point_from_rectangles(&rects, RectangleEdge::Right, CaretQuality::TextNeighbor)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_range_neighbor_uses_character_edge_without_changing_direct_range() {
        let rect = [100.0, 200.0, 12.0, 20.0];
        let direct =
            point_from_rectangles(&rect, RectangleEdge::Left, CaretQuality::Reliable).unwrap();
        let neighbor =
            point_from_rectangles(&rect, RectangleEdge::Right, CaretQuality::TextNeighbor).unwrap();
        assert_eq!((direct.x, direct.y), (100, 220));
        assert_eq!((neighbor.x, neighbor.y), (112, 220));
        assert!(!neighbor.is_reliable());
        assert!(
            point_from_rectangles(
                &[100.0, 200.0, 0.0, 20.0],
                RectangleEdge::Left,
                CaretQuality::Reliable
            )
            .is_some()
        );
        assert!(
            point_from_rectangles(
                &[100.0, 200.0, 0.0, 20.0],
                RectangleEdge::Right,
                CaretQuality::TextNeighbor
            )
            .is_none()
        );
    }
}
