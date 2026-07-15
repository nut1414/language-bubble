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
