use crate::caret::{CaretQuality, ScreenPoint, ScreenRect};
use crate::types::{DisplayMode, SizeMetrics};

const ESTIMATED_LINE_HEIGHT_DIP: f32 = 20.0;

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
    pub foreground_bottom: Option<i32>,
    pub window_size: PixelSize,
    pub dpi_scale: f32,
    pub metrics: SizeMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretAnchor {
    Center,
    SelectedItem(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementPlan {
    CenterOnScreen,
    AtCaret(CaretAnchor),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionPlan {
    FadeIn { slide_offset: f32 },
    CarouselSlide { from: f32, to: f32 },
    ExpandedWindowSlide { horizontal_offset: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubbleShowPlan {
    pub window_size: PixelSize,
    pub placement: PlacementPlan,
    pub transition: TransitionPlan,
    pub capture_label_opacities: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct BubbleShowInput {
    pub metrics: SizeMetrics,
    pub display_mode: DisplayMode,
    pub label_count: usize,
    pub selected: i32,
    pub previous_selected: i32,
    pub caret_available: bool,
    pub dpi_scale: f32,
}

pub fn calculate_show_plan(input: BubbleShowInput) -> BubbleShowPlan {
    let can_slide = input.previous_selected >= 0
        && input.previous_selected != input.selected
        && input.label_count > 1
        && input.caret_available;
    let placement = if !input.caret_available {
        PlacementPlan::CenterOnScreen
    } else if input.display_mode == DisplayMode::Expanded && input.label_count > 1 {
        PlacementPlan::AtCaret(CaretAnchor::SelectedItem(input.selected))
    } else {
        PlacementPlan::AtCaret(CaretAnchor::Center)
    };
    let transition = if can_slide && input.display_mode == DisplayMode::Expanded {
        let delta = input.selected - input.previous_selected;
        TransitionPlan::ExpandedWindowSlide {
            horizontal_offset: (delta as f32 * input.metrics.item_width * input.dpi_scale) as i32,
        }
    } else if can_slide && input.display_mode == DisplayMode::Carousel {
        TransitionPlan::CarouselSlide {
            from: -input.previous_selected as f32 * input.metrics.item_width,
            to: -input.selected as f32 * input.metrics.item_width,
        }
    } else {
        TransitionPlan::FadeIn {
            slide_offset: -input.selected as f32 * input.metrics.item_width,
        }
    };

    BubbleShowPlan {
        window_size: calculate_window_size(
            input.metrics,
            input.display_mode,
            input.label_count,
            input.dpi_scale,
        ),
        placement,
        transition,
        capture_label_opacities: can_slide,
    }
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
    let unreliable_caret = context.caret.quality == CaretQuality::Unreliable;
    let field_fallback = context.caret.quality.is_field_fallback();
    let element = context.caret.element_rect.filter(|_| field_fallback);
    let bottom_limit = context
        .foreground_bottom
        .filter(|bottom| field_fallback && *bottom > context.work_area.top)
        .map_or(context.work_area.bottom, |bottom| {
            bottom.min(context.work_area.bottom)
        });
    let mut x = match anchor {
        CaretAnchor::Center => context.caret.x - context.window_size.width / 2,
        CaretAnchor::SelectedItem(selected) => {
            let selected_center_dip = context.metrics.padding
                + selected as f32 * context.metrics.item_width
                + context.metrics.item_width / 2.0;
            context.caret.x - (selected_center_dip * context.dpi_scale) as i32
        }
    };
    let below = if unreliable_caret {
        element.map(|rect| rect.bottom).unwrap_or_else(|| {
            context.caret.caret_top + (ESTIMATED_LINE_HEIGHT_DIP * context.dpi_scale) as i32
        }) + caret_offset
    } else {
        context.caret.y + caret_offset
    };
    let above = element
        .map(|rect| rect.top)
        .unwrap_or(context.caret.caret_top)
        - context.window_size.height
        - caret_offset;
    let fits_below = |y| y + context.window_size.height <= bottom_limit - margin;

    if x + context.window_size.width > context.work_area.right - margin {
        x = context.work_area.right - context.window_size.width - margin;
    }
    if x < context.work_area.left + margin {
        x = context.work_area.left + margin;
    }
    let mut y = if fits_below(below) { below } else { above };

    if let Some(element_rect) = element
        && rectangles_overlap(x, y, context.window_size, element_rect)
    {
        let below_element = element_rect.bottom + caret_offset;
        y = if fits_below(below_element) {
            below_element
        } else {
            above
        };
    }

    PixelPoint {
        x,
        y: y.max(context.work_area.top + margin),
    }
}

fn rectangles_overlap(x: i32, y: i32, size: PixelSize, rect: ScreenRect) -> bool {
    let right = x as i64 + size.width as i64;
    let bottom = y as i64 + size.height as i64;
    right > rect.left as i64
        && (x as i64) < rect.right as i64
        && bottom > rect.top as i64
        && (y as i64) < rect.bottom as i64
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

    fn reliable_caret(x: i32, y: i32, caret_top: i32) -> ScreenPoint {
        ScreenPoint {
            x,
            y,
            caret_top,
            quality: CaretQuality::Reliable,
            element_rect: None,
        }
    }

    fn context(
        caret: ScreenPoint,
        work_area: WorkArea,
        window_size: PixelSize,
        dpi_scale: f32,
    ) -> PlacementContext {
        PlacementContext {
            caret,
            work_area,
            foreground_bottom: None,
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
                reliable_caret(500, 400, 380),
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
    fn tiny_caret_uses_element_bottom_without_changing_x() {
        let point = place_at_caret(
            context(
                ScreenPoint {
                    x: 500,
                    y: 382,
                    caret_top: 380,
                    quality: CaretQuality::Unreliable,
                    element_rect: Some(ScreenRect {
                        left: 450,
                        top: 380,
                        right: 550,
                        bottom: 420,
                    }),
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
        assert_eq!(point, PixelPoint { x: 450, y: 424 });
    }

    #[test]
    fn tiny_caret_without_element_bounds_uses_estimated_line_height() {
        let point = place_at_caret(
            context(
                ScreenPoint {
                    x: 500,
                    y: 382,
                    caret_top: 380,
                    quality: CaretQuality::Unreliable,
                    element_rect: None,
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
    fn tiny_caret_flips_above_element_at_work_area_bottom() {
        let point = place_at_caret(
            context(
                ScreenPoint {
                    x: 500,
                    y: 1002,
                    caret_top: 1000,
                    quality: CaretQuality::Unreliable,
                    element_rect: Some(ScreenRect {
                        left: 450,
                        top: 1000,
                        right: 550,
                        bottom: 1040,
                    }),
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
        assert_eq!(point, PixelPoint { x: 450, y: 956 });
    }

    #[test]
    fn field_fallback_stays_within_a_floating_app_window() {
        for quality in [CaretQuality::Unreliable, CaretQuality::EditFallback] {
            let mut placement = context(
                ScreenPoint {
                    x: 60,
                    y: 715,
                    caret_top: 713,
                    quality,
                    element_rect: Some(ScreenRect {
                        left: 40,
                        top: 713,
                        right: 600,
                        bottom: 894,
                    }),
                },
                WorkArea {
                    left: 0,
                    top: 0,
                    right: 1200,
                    bottom: 978,
                },
                PixelSize {
                    width: 64,
                    height: 55,
                },
                1.0,
            );
            placement.foreground_bottom = Some(894);
            assert_eq!(
                place_at_caret(placement, CaretAnchor::Center),
                PixelPoint { x: 28, y: 654 }
            );
        }
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
                context(reliable_caret(0, 400, 380), work_area, size, 1.0,),
                CaretAnchor::Center,
            ),
            PixelPoint { x: 10, y: 404 }
        );
        assert_eq!(
            place_at_caret(
                context(reliable_caret(1900, 400, 380), work_area, size, 1.0,),
                CaretAnchor::Center,
            ),
            PixelPoint { x: 1810, y: 404 }
        );
        assert_eq!(
            place_at_caret(
                context(reliable_caret(500, 1075, 1055), work_area, size, 1.0,),
                CaretAnchor::Center,
            ),
            PixelPoint { x: 450, y: 1011 }
        );
        assert_eq!(
            place_at_caret(
                context(reliable_caret(500, 2, 0), work_area, size, 1.0,),
                CaretAnchor::Center,
            ),
            PixelPoint { x: 450, y: 10 }
        );
    }

    #[test]
    fn selected_item_anchor_uses_scaled_item_center() {
        let point = place_at_caret(
            context(
                reliable_caret(500, 400, 380),
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
                context(reliable_caret(-1800, 400, 380), work_area, size, 1.0,),
                CaretAnchor::Center,
            ),
            PixelPoint { x: -1850, y: 404 }
        );
    }

    #[test]
    fn oversized_window_keeps_existing_clamp_order() {
        let point = place_at_caret(
            context(
                reliable_caret(50, 50, 30),
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

    fn show_input(display_mode: DisplayMode) -> BubbleShowInput {
        BubbleShowInput {
            metrics: BubbleSize::Medium.metrics(),
            display_mode,
            label_count: 3,
            selected: 1,
            previous_selected: 0,
            caret_available: true,
            dpi_scale: 1.0,
        }
    }

    #[test]
    fn first_show_fades_in_at_the_caret() {
        let mut input = show_input(DisplayMode::Carousel);
        input.previous_selected = -1;
        let plan = calculate_show_plan(input);
        assert_eq!(
            plan.transition,
            TransitionPlan::FadeIn {
                slide_offset: -30.0,
            }
        );
        assert_eq!(plan.placement, PlacementPlan::AtCaret(CaretAnchor::Center));
        assert!(!plan.capture_label_opacities);
    }

    #[test]
    fn carousel_selection_change_uses_row_slide() {
        let plan = calculate_show_plan(show_input(DisplayMode::Carousel));
        assert_eq!(
            plan.transition,
            TransitionPlan::CarouselSlide {
                from: 0.0,
                to: -30.0,
            }
        );
        assert!(plan.capture_label_opacities);
    }

    #[test]
    fn expanded_selection_change_uses_scaled_window_slide() {
        let mut input = show_input(DisplayMode::Expanded);
        input.selected = 2;
        input.dpi_scale = 1.5;
        let plan = calculate_show_plan(input);
        assert_eq!(
            plan.transition,
            TransitionPlan::ExpandedWindowSlide {
                horizontal_offset: 90,
            }
        );
        assert_eq!(
            plan.placement,
            PlacementPlan::AtCaret(CaretAnchor::SelectedItem(2))
        );
        assert!(plan.capture_label_opacities);
    }

    #[test]
    fn missing_caret_disables_slide_and_centers_window() {
        let mut input = show_input(DisplayMode::Carousel);
        input.caret_available = false;
        let plan = calculate_show_plan(input);
        assert_eq!(plan.placement, PlacementPlan::CenterOnScreen);
        assert!(matches!(plan.transition, TransitionPlan::FadeIn { .. }));
        assert!(!plan.capture_label_opacities);
    }

    #[test]
    fn simple_mode_keeps_fade_transition_after_selection_change() {
        let plan = calculate_show_plan(show_input(DisplayMode::Simple));
        assert!(matches!(plan.transition, TransitionPlan::FadeIn { .. }));
        assert!(plan.capture_label_opacities);
    }

    #[test]
    fn single_expanded_label_uses_normal_anchor_and_size() {
        let mut input = show_input(DisplayMode::Expanded);
        input.label_count = 1;
        let plan = calculate_show_plan(input);
        assert_eq!(plan.placement, PlacementPlan::AtCaret(CaretAnchor::Center));
        assert_eq!(
            plan.window_size,
            PixelSize {
                width: 43,
                height: 37,
            }
        );
        assert!(matches!(plan.transition, TransitionPlan::FadeIn { .. }));
    }
}
