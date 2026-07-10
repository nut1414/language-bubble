use crate::caret::ScreenPoint;
use crate::types::{DisplayMode, SizeMetrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct PlacementContext {
    pub caret: ScreenPoint,
    pub work_area: WorkArea,
    pub window_size: PixelSize,
    pub dpi_scale: f32,
    pub metrics: SizeMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretAnchor {
    Center,
    SelectedItem(i32),
}

pub fn calculate_window_size(
    metrics: SizeMetrics,
    display_mode: DisplayMode,
    label_count: usize,
    dpi_scale: f32,
) -> PixelSize {
    let count = label_count as f32;
    let content_width = match display_mode {
        DisplayMode::Expanded if label_count > 1 => count * metrics.item_width,
        _ => metrics.item_width,
    };
    PixelSize {
        width: ((content_width + metrics.padding * 2.0 + 1.0) * dpi_scale) as i32,
        height: ((metrics.item_height + metrics.padding * 2.0 + 1.0) * dpi_scale) as i32,
    }
}

pub fn place_at_caret(context: PlacementContext, anchor: CaretAnchor) -> PixelPoint {
    let margin = (10.0 * context.dpi_scale) as i32;
    let caret_offset = (4.0 * context.dpi_scale) as i32;
    let mut x = match anchor {
        CaretAnchor::Center => context.caret.x - context.window_size.width / 2,
        CaretAnchor::SelectedItem(selected) => {
            let selected_center_dip = context.metrics.padding
                + selected as f32 * context.metrics.item_width
                + context.metrics.item_width / 2.0;
            context.caret.x - (selected_center_dip * context.dpi_scale) as i32
        }
    };
    let mut y = context.caret.y + caret_offset;

    if x + context.window_size.width > context.work_area.right - margin {
        x = context.work_area.right - context.window_size.width - margin;
    }
    if x < context.work_area.left + margin {
        x = context.work_area.left + margin;
    }
    if y + context.window_size.height > context.work_area.bottom - margin {
        y = context.caret.caret_top - context.window_size.height - caret_offset;
    }
    if y < context.work_area.top + margin {
        y = context.work_area.top + margin;
    }

    PixelPoint { x, y }
}

pub fn center_in_work_area(work_area: WorkArea, window_size: PixelSize) -> PixelPoint {
    let work_width = work_area.right - work_area.left;
    let work_height = work_area.bottom - work_area.top;
    PixelPoint {
        x: work_area.left + (work_width - window_size.width) / 2,
        y: work_area.top + (work_height - window_size.height) / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BubbleSize;

    fn context(
        caret: ScreenPoint,
        work_area: WorkArea,
        window_size: PixelSize,
        dpi_scale: f32,
    ) -> PlacementContext {
        PlacementContext {
            caret,
            work_area,
            window_size,
            dpi_scale,
            metrics: BubbleSize::Medium.metrics(),
        }
    }

    #[test]
    fn window_size_preserves_dip_to_pixel_truncation() {
        let metrics = BubbleSize::Medium.metrics();
        assert_eq!(
            calculate_window_size(metrics, DisplayMode::Carousel, 3, 1.0),
            PixelSize {
                width: 43,
                height: 37,
            }
        );
        assert_eq!(
            calculate_window_size(metrics, DisplayMode::Expanded, 3, 1.5),
            PixelSize {
                width: 154,
                height: 55,
            }
        );
        assert_eq!(
            calculate_window_size(metrics, DisplayMode::Expanded, 1, 2.0),
            PixelSize {
                width: 86,
                height: 74,
            }
        );
    }

    #[test]
    fn caret_placement_centers_below_the_caret() {
        let point = place_at_caret(
            context(
                ScreenPoint {
                    x: 500,
                    y: 400,
                    caret_top: 380,
                },
                WorkArea {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                PixelSize {
                    width: 100,
                    height: 40,
                },
                1.0,
            ),
            CaretAnchor::Center,
        );
        assert_eq!(point, PixelPoint { x: 450, y: 404 });
    }

    #[test]
    fn caret_placement_clamps_every_screen_edge() {
        let work_area = WorkArea {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let size = PixelSize {
            width: 100,
            height: 40,
        };
        assert_eq!(
            place_at_caret(
                context(
                    ScreenPoint {
                        x: 0,
                        y: 400,
                        caret_top: 380,
                    },
                    work_area,
                    size,
                    1.0,
                ),
                CaretAnchor::Center,
            ),
            PixelPoint { x: 10, y: 404 }
        );
        assert_eq!(
            place_at_caret(
                context(
                    ScreenPoint {
                        x: 1900,
                        y: 400,
                        caret_top: 380,
                    },
                    work_area,
                    size,
                    1.0,
                ),
                CaretAnchor::Center,
            ),
            PixelPoint { x: 1810, y: 404 }
        );
        assert_eq!(
            place_at_caret(
                context(
                    ScreenPoint {
                        x: 500,
                        y: 1075,
                        caret_top: 1055,
                    },
                    work_area,
                    size,
                    1.0,
                ),
                CaretAnchor::Center,
            ),
            PixelPoint { x: 450, y: 1011 }
        );
        assert_eq!(
            place_at_caret(
                context(
                    ScreenPoint {
                        x: 500,
                        y: 2,
                        caret_top: 0,
                    },
                    work_area,
                    size,
                    1.0,
                ),
                CaretAnchor::Center,
            ),
            PixelPoint { x: 450, y: 10 }
        );
    }

    #[test]
    fn selected_item_anchor_uses_scaled_item_center() {
        let point = place_at_caret(
            context(
                ScreenPoint {
                    x: 500,
                    y: 400,
                    caret_top: 380,
                },
                WorkArea {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                PixelSize {
                    width: 200,
                    height: 40,
                },
                1.5,
            ),
            CaretAnchor::SelectedItem(2),
        );
        assert_eq!(point, PixelPoint { x: 379, y: 406 });
    }

    #[test]
    fn negative_monitor_coordinates_and_centering_are_preserved() {
        let work_area = WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let size = PixelSize {
            width: 100,
            height: 40,
        };
        assert_eq!(
            center_in_work_area(work_area, size),
            PixelPoint { x: -1010, y: 520 }
        );
        assert_eq!(
            place_at_caret(
                context(
                    ScreenPoint {
                        x: -1800,
                        y: 400,
                        caret_top: 380,
                    },
                    work_area,
                    size,
                    1.0,
                ),
                CaretAnchor::Center,
            ),
            PixelPoint { x: -1850, y: 404 }
        );
    }

    #[test]
    fn oversized_window_keeps_existing_clamp_order() {
        let point = place_at_caret(
            context(
                ScreenPoint {
                    x: 50,
                    y: 50,
                    caret_top: 30,
                },
                WorkArea {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 100,
                },
                PixelSize {
                    width: 150,
                    height: 40,
                },
                1.0,
            ),
            CaretAnchor::Center,
        );
        assert_eq!(point.x, 10);
    }
}
