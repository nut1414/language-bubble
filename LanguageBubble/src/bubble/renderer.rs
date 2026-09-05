use crate::animation::AnimController;
use crate::types::*;
use std::mem;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::*;

// Dark mode colors
const DARK_BG: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0x2D as f32 / 255.0,
    g: 0x2D as f32 / 255.0,
    b: 0x2D as f32 / 255.0,
    a: 0xDD as f32 / 255.0,
};
const DARK_BORDER: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 0x44 as f32 / 255.0,
};

// Light mode colors
const LIGHT_BG: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0xF3 as f32 / 255.0,
    g: 0xF3 as f32 / 255.0,
    b: 0xF3 as f32 / 255.0,
    a: 0xDD as f32 / 255.0,
};
const LIGHT_BORDER: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0x44 as f32 / 255.0,
};

pub(super) struct Frame<'a> {
    pub size: BubbleSize,
    pub anim: &'a AnimController,
    pub theme_mode: ThemeMode,
    pub custom_colors: CustomThemeColors,
    pub dark_mode: bool,
    pub display_mode: DisplayMode,
    pub labels: &'a [String],
    pub selected_index: i32,
}

impl Frame<'_> {
    /// Get the *target* opacity for a label (what it should settle at).
    pub(super) fn get_label_target_opacity(&self, index: i32) -> f32 {
        if self.labels.len() <= 1 {
            return 1.0;
        }
        if self.display_mode == DisplayMode::Simple {
            return if index == self.selected_index {
                1.0
            } else {
                0.0
            };
        }
        if index == self.selected_index {
            1.0
        } else {
            0.3
        }
    }

    /// Get the current animated opacity for a label.
    fn get_label_opacity(&self, index: i32) -> f32 {
        let target = self.get_label_target_opacity(index);
        // Let the animation controller interpolate from previous snapshot
        self.anim.label_opacity(index as usize, target)
    }
}

pub(super) struct Renderer {
    d2d_factory: ID2D1Factory,
    dwrite_factory: IDWriteFactory,
    render_target: Option<ID2D1HwndRenderTarget>,
    text_format: Option<IDWriteTextFormat>,
    text_layouts: Vec<Option<IDWriteTextLayout>>,
}

impl Renderer {
    pub(super) fn new() -> windows::core::Result<Self> {
        let d2d_factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let dwrite_factory: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };

        Ok(Self {
            d2d_factory,
            dwrite_factory,
            render_target: None,
            text_format: None,
            text_layouts: Vec::new(),
        })
    }
    pub(super) fn invalidate(&mut self) {
        self.render_target = None;
    }
    pub(super) fn create_text_format(&mut self, size: BubbleSize) {
        let metrics = size.metrics();
        unsafe {
            self.text_format = self
                .dwrite_factory
                .CreateTextFormat(
                    w!("Segoe UI Semibold"),
                    None,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    metrics.font_size,
                    w!("en-us"),
                )
                .ok();
            if let Some(ref fmt) = self.text_format {
                let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            }
        }
    }

    pub(super) fn rebuild_text_layouts(&mut self, labels: &[String], size: BubbleSize) {
        self.text_layouts = labels
            .iter()
            .map(|label| self.create_fitted_text_layout(label, size).ok())
            .collect();
    }

    fn create_fitted_text_layout(
        &self,
        label: &str,
        size: BubbleSize,
    ) -> windows::core::Result<IDWriteTextLayout> {
        let Some(text_format) = &self.text_format else {
            return Err(windows::core::Error::from_hresult(E_FAIL));
        };
        let metrics = size.metrics();
        let wide: Vec<u16> = label.encode_utf16().collect();
        let layout = unsafe {
            self.dwrite_factory.CreateTextLayout(
                &wide,
                text_format,
                metrics.item_width * 8.0,
                metrics.item_height,
            )?
        };

        unsafe {
            let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            let _ = layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

            let mut text_metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut text_metrics)?;
            let scale = fitted_font_scale(
                text_metrics.widthIncludingTrailingWhitespace,
                metrics.item_width * 0.9,
            );
            if scale < 1.0 {
                layout.SetFontSize(
                    metrics.font_size * scale,
                    DWRITE_TEXT_RANGE {
                        startPosition: 0,
                        length: wide.len() as u32,
                    },
                )?;
            }
            layout.SetMaxWidth(metrics.item_width)?;
            layout.SetMaxHeight(metrics.item_height)?;
        }

        Ok(layout)
    }

    fn ensure_render_target(&mut self, hwnd: HWND) {
        if self.render_target.is_some() {
            return;
        }
        unsafe {
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let size = D2D_SIZE_U {
                width: (rc.right - rc.left).max(1) as u32,
                height: (rc.bottom - rc.top).max(1) as u32,
            };
            // Use actual monitor DPI so D2D correctly scales DIP-based
            // drawing coordinates (fonts, padding, radii) to physical pixels.
            let dpi = GetDpiForWindow(hwnd) as f32;
            let dpi = if dpi > 0.0 { dpi } else { 96.0 };
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi,
                dpiY: dpi,
                ..Default::default()
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: size,
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            self.render_target = self
                .d2d_factory
                .CreateHwndRenderTarget(&props, &hwnd_props)
                .ok();
        }
    }

    pub(super) fn render(&mut self, hwnd: HWND, frame: Frame<'_>) {
        self.ensure_render_target(hwnd);
        let result = match (&self.render_target, &self.text_format) {
            (Some(render_target), Some(text_format)) => {
                self.draw_frame(render_target, text_format, &frame)
            }
            _ => return,
        };

        // EndDraw reports device loss through D2DERR_RECREATE_TARGET. Dropping
        // all target-dependent resources lets the next animation tick recover.
        if result.is_err() {
            self.render_target = None;
        }
    }

    #[allow(clippy::missing_transmute_annotations)]
    fn draw_frame(
        &self,
        rt: &ID2D1HwndRenderTarget,
        fmt: &IDWriteTextFormat,
        frame: &Frame<'_>,
    ) -> windows::core::Result<()> {
        let metrics = frame.size.metrics();
        let opacity = frame.anim.opacity();
        let (bg_color, border_color, fg_base) = match frame.theme_mode {
            ThemeMode::Custom => {
                let bg_rgb = frame.custom_colors.bg_color;
                let bg_color = D2D1_COLOR_F {
                    r: (bg_rgb & 0xFF) as f32 / 255.0,
                    g: ((bg_rgb >> 8) & 0xFF) as f32 / 255.0,
                    b: ((bg_rgb >> 16) & 0xFF) as f32 / 255.0,
                    a: frame.custom_colors.opacity as f32 / 255.0,
                };
                let fg_rgb = frame.custom_colors.fg_color;
                let fg_base = D2D1_COLOR_F {
                    r: (fg_rgb & 0xFF) as f32 / 255.0,
                    g: ((fg_rgb >> 8) & 0xFF) as f32 / 255.0,
                    b: ((fg_rgb >> 16) & 0xFF) as f32 / 255.0,
                    a: 1.0,
                };
                let border_color = D2D1_COLOR_F {
                    r: fg_base.r,
                    g: fg_base.g,
                    b: fg_base.b,
                    a: 0x44 as f32 / 255.0,
                };
                (bg_color, border_color, fg_base)
            }
            _ => {
                if frame.dark_mode {
                    (
                        DARK_BG,
                        DARK_BORDER,
                        D2D1_COLOR_F {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        },
                    )
                } else {
                    (
                        LIGHT_BG,
                        LIGHT_BORDER,
                        D2D1_COLOR_F {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        },
                    )
                }
            }
        };

        unsafe {
            rt.BeginDraw();
            let draw_result = (|| -> windows::core::Result<()> {
                rt.Clear(Some(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }));

                let size = rt.GetSize();

                // Background rounded rect
                let bg_brush = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        a: bg_color.a * opacity,
                        ..bg_color
                    },
                    None,
                )?;
                let rrect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 0.5,
                        top: 0.5,
                        right: size.width - 0.5,
                        bottom: size.height - 0.5,
                    },
                    radiusX: metrics.corner_radius,
                    radiusY: metrics.corner_radius,
                };
                rt.FillRoundedRectangle(&rrect, &bg_brush);

                // Border
                let border_brush = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        a: border_color.a * opacity,
                        ..border_color
                    },
                    None,
                )?;
                rt.DrawRoundedRectangle(&rrect, &border_brush, 0.5, None);

                // Draw labels
                let slide_offset = if frame.display_mode == DisplayMode::Carousel
                    && frame.labels.len() > 1
                {
                    frame.anim.slide_offset()
                } else if frame.display_mode == DisplayMode::Expanded && frame.labels.len() > 1 {
                    0.0 // All labels visible, no row offset
                } else {
                    -(frame.selected_index as f32 * metrics.item_width)
                };

                for (i, label_text) in frame.labels.iter().enumerate() {
                    let label_opacity = frame.get_label_opacity(i as i32);
                    let fg_color = D2D1_COLOR_F {
                        a: label_opacity * opacity,
                        ..fg_base
                    };
                    let fg_brush = rt.CreateSolidColorBrush(&fg_color, None)?;

                    let x = metrics.padding + i as f32 * metrics.item_width + slide_offset;
                    let y = metrics.padding;

                    let rect = D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + metrics.item_width,
                        bottom: y + metrics.item_height,
                    };

                    if let Some(Some(text_layout)) = self.text_layouts.get(i) {
                        rt.DrawTextLayout(
                            mem::transmute([x, y]),
                            text_layout,
                            &fg_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                        );
                    } else {
                        let wide: Vec<u16> = label_text.encode_utf16().collect();
                        rt.DrawText(
                            &wide,
                            fmt,
                            &rect,
                            &fg_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                }

                Ok(())
            })();
            let end_result = rt.EndDraw(None, None);
            draw_result.and(end_result)?;
        }
        Ok(())
    }
}

fn fitted_font_scale(natural_width: f32, available_width: f32) -> f32 {
    if natural_width <= 0.0 || natural_width <= available_width {
        1.0
    } else {
        (available_width / natural_width).clamp(0.1, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_font_scaling_only_shrinks_overflowing_text() {
        assert_eq!(fitted_font_scale(20.0, 27.0), 1.0);
        assert_eq!(fitted_font_scale(0.0, 27.0), 1.0);
        assert!((fitted_font_scale(54.0, 27.0) - 0.5).abs() < f32::EPSILON);
        assert_eq!(fitted_font_scale(1000.0, 27.0), 0.1);
    }
}
