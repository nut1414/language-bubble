use std::mem;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::animation::*;
use crate::bubble_layout::{
    BubbleShowInput, PixelPoint, PixelSize, PlacementContext, PlacementPlan, TransitionPlan,
    WorkArea, calculate_show_plan, center_in_work_area, place_at_caret,
};
use crate::caret::ScreenPoint;
use crate::language::LayoutInfo;
use crate::registry::RegistryKey;
use crate::types::*;

const CLASS_NAME: PCWSTR = w!("LanguageBubbleOverlay");
const DPI_AWARENESS_CONTEXT_PMV2: isize = -4;

// Timer IDs
pub const TIMER_HIDE: usize = 1;
pub const TIMER_TOPMOST: usize = 2;
pub const TIMER_ANIM: usize = 3;

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

pub struct BubbleWindow {
    pub hwnd: HWND,
    /// The message-only window that handles WM_TIMER dispatching.
    msg_hwnd: HWND,
    d2d_factory: ID2D1Factory,
    dwrite_factory: IDWriteFactory,
    render_target: Option<ID2D1HwndRenderTarget>,
    text_format: Option<IDWriteTextFormat>,
    pub anim: AnimController,
    pub size: BubbleSize,
    pub display_mode: DisplayMode,
    pub labels: Vec<String>,
    pub selected_index: i32,
    pub previous_selected_index: i32,
    dark_mode: bool,
    theme_mode: ThemeMode,
    custom_colors: CustomThemeColors,
    desired_phys_x: i32,
    desired_phys_y: i32,
}

impl BubbleWindow {
    pub fn new(msg_hwnd: HWND) -> windows::core::Result<Self> {
        let d2d_factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let dwrite_factory: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };

        let hwnd = create_overlay_window()?;

        let mut bw = Self {
            hwnd,
            msg_hwnd,
            d2d_factory,
            dwrite_factory,
            render_target: None,
            text_format: None,
            anim: AnimController::new(),
            size: BubbleSize::Medium,
            display_mode: DisplayMode::Carousel,
            labels: Vec::new(),
            selected_index: -1,
            previous_selected_index: -1,
            dark_mode: is_dark_mode(),
            theme_mode: ThemeMode::System,
            custom_colors: CustomThemeColors::default(),
            desired_phys_x: 0,
            desired_phys_y: 0,
        };
        bw.create_text_format();
        Ok(bw)
    }

    pub fn set_size(&mut self, size: BubbleSize) {
        self.size = size;
        self.create_text_format();
        self.render_target = None;
    }

    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
        self.dark_mode = resolve_dark_mode(mode);
    }

    pub fn set_custom_colors(&mut self, colors: CustomThemeColors) {
        self.custom_colors = colors;
    }

    pub fn refresh_theme(&mut self) {
        self.dark_mode = resolve_dark_mode(self.theme_mode);
    }

    fn create_text_format(&mut self) {
        let metrics = self.size.metrics();
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
            }
        }
    }

    fn ensure_render_target(&mut self) {
        if self.render_target.is_some() {
            return;
        }
        unsafe {
            let mut rc = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut rc);
            let size = D2D_SIZE_U {
                width: (rc.right - rc.left).max(1) as u32,
                height: (rc.bottom - rc.top).max(1) as u32,
            };
            // Use actual monitor DPI so D2D correctly scales DIP-based
            // drawing coordinates (fonts, padding, radii) to physical pixels.
            let dpi = GetDpiForWindow(self.hwnd) as f32;
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
                hwnd: self.hwnd,
                pixelSize: size,
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            self.render_target = self
                .d2d_factory
                .CreateHwndRenderTarget(&props, &hwnd_props)
                .ok();
        }
    }

    pub fn show_bubble(
        &mut self,
        layouts: &[LayoutInfo],
        selected: i32,
        caret: Option<ScreenPoint>,
    ) {
        self.stop_show_timers();
        self.labels = layouts.iter().map(|l| l.bubble_text.clone()).collect();
        let monitor = self.destination_monitor(caret);
        let dpi_scale = self.monitor_dpi_scale(monitor);
        let plan = calculate_show_plan(BubbleShowInput {
            metrics: self.size.metrics(),
            display_mode: self.display_mode,
            label_count: self.labels.len(),
            selected,
            previous_selected: self.previous_selected_index,
            caret_available: caret.is_some(),
            dpi_scale,
        });

        if plan.capture_label_opacities {
            let current_opacities: Vec<f32> = (0..self.labels.len())
                .map(|i| {
                    self.anim
                        .label_opacity(i, self.get_label_target_opacity(i as i32))
                })
                .collect();
            self.anim.begin_label_transition(current_opacities);
        }
        self.selected_index = selected;
        self.resize_window(plan.window_size);
        let target =
            self.target_position(plan.placement, caret, plan.window_size, monitor, dpi_scale);
        self.apply_transition(plan.transition, target);
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        self.render();
        self.previous_selected_index = selected;
        self.start_show_timers();
    }

    fn stop_show_timers(&self) {
        unsafe {
            let _ = KillTimer(Some(self.msg_hwnd), TIMER_HIDE);
            let _ = KillTimer(Some(self.msg_hwnd), TIMER_ANIM);
        }
    }

    fn resize_window(&mut self, size: PixelSize) {
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                size.width,
                size.height,
                SWP_NOMOVE | SWP_NOACTIVATE,
            )
            .ok();
        }
        self.render_target = None;
    }

    fn apply_transition(&mut self, transition: TransitionPlan, target: PixelPoint) {
        match transition {
            TransitionPlan::FadeIn { slide_offset } => {
                self.set_physical_position(target.x, target.y);
                self.anim.slide_to = slide_offset;
                self.anim.slide_from = slide_offset;
                self.anim.sliding = false;
                self.anim.begin_fade_in();
            }
            TransitionPlan::CarouselSlide { from, to } => {
                self.set_physical_position(target.x, target.y);
                self.anim.begin_slide(from, to);
                self.anim.set_visible();
            }
            TransitionPlan::ExpandedWindowSlide { horizontal_offset } => {
                self.anim
                    .begin_window_slide(target.x + horizontal_offset, target.x);
                let start_x = self.anim.win_slide_from_x;
                self.desired_phys_x = target.x;
                self.desired_phys_y = target.y;
                unsafe {
                    let _ = SetWindowPos(
                        self.hwnd,
                        Some(HWND_TOPMOST),
                        start_x,
                        target.y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
                self.anim.set_visible();
            }
        }
    }

    fn start_show_timers(&self) {
        unsafe {
            SetTimer(Some(self.msg_hwnd), TIMER_HIDE, 1500, None);
            SetTimer(Some(self.msg_hwnd), TIMER_TOPMOST, 100, None);
            SetTimer(Some(self.msg_hwnd), TIMER_ANIM, 8, None);
        }
    }

    pub fn instant_hide(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.msg_hwnd), TIMER_HIDE);
            let _ = KillTimer(Some(self.msg_hwnd), TIMER_TOPMOST);
            let _ = KillTimer(Some(self.msg_hwnd), TIMER_ANIM);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.anim = AnimController::new();
    }

    pub fn begin_hide(&mut self) {
        self.anim.begin_fade_out();
    }

    pub fn tick(&mut self) {
        // Update animations
        if self.anim.is_fade_complete() && self.anim.state == AnimState::FadeIn {
            self.anim.set_visible();
        }
        if self.anim.is_fade_complete() && self.anim.state == AnimState::FadeOut {
            self.anim.state = AnimState::Idle;
            self.instant_hide();
            return;
        }
        if self.anim.sliding && self.anim.is_slide_complete() {
            self.anim.finish_slide();
        }
        if self.anim.win_sliding {
            let target_x = self.desired_phys_x;
            let x = self.anim.window_slide_x();
            // Move window without overwriting the target in desired_phys_x
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    Some(HWND_TOPMOST),
                    x,
                    self.desired_phys_y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            self.desired_phys_x = target_x; // preserve target
            if self.anim.is_window_slide_complete() {
                self.anim.finish_window_slide();
                // Snap to exact target
                self.set_physical_position(target_x, self.desired_phys_y);
            }
        }

        self.render();
    }

    pub fn refresh_topmost(&self) {
        unsafe {
            if !self.hwnd.is_invalid() {
                let _ = SetWindowPos(
                    self.hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }

    fn render(&mut self) {
        self.ensure_render_target();
        let result = match (&self.render_target, &self.text_format) {
            (Some(render_target), Some(text_format)) => self.draw_frame(render_target, text_format),
            _ => return,
        };

        // EndDraw reports device loss through D2DERR_RECREATE_TARGET. Dropping
        // all target-dependent resources lets the next animation tick recover.
        if result.is_err() {
            self.render_target = None;
        }
    }

    fn draw_frame(
        &self,
        rt: &ID2D1HwndRenderTarget,
        fmt: &IDWriteTextFormat,
    ) -> windows::core::Result<()> {
        let metrics = self.size.metrics();
        let opacity = self.anim.opacity();
        let (bg_color, border_color, fg_base) = match self.theme_mode {
            ThemeMode::Custom => {
                let bg_rgb = self.custom_colors.bg_color;
                let bg_color = D2D1_COLOR_F {
                    r: (bg_rgb & 0xFF) as f32 / 255.0,
                    g: ((bg_rgb >> 8) & 0xFF) as f32 / 255.0,
                    b: ((bg_rgb >> 16) & 0xFF) as f32 / 255.0,
                    a: self.custom_colors.opacity as f32 / 255.0,
                };
                let fg_rgb = self.custom_colors.fg_color;
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
                if self.dark_mode {
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
                let slide_offset =
                    if self.display_mode == DisplayMode::Carousel && self.labels.len() > 1 {
                        self.anim.slide_offset()
                    } else if self.display_mode == DisplayMode::Expanded && self.labels.len() > 1 {
                        0.0 // All labels visible, no row offset
                    } else {
                        -(self.selected_index as f32 * metrics.item_width)
                    };

                for (i, label_text) in self.labels.iter().enumerate() {
                    let label_opacity = self.get_label_opacity(i as i32);
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

                Ok(())
            })();
            let end_result = rt.EndDraw(None, None);
            draw_result.and(end_result)?;
        }
        Ok(())
    }

    /// Get the *target* opacity for a label (what it should settle at).
    fn get_label_target_opacity(&self, index: i32) -> f32 {
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

    fn get_dpi_scale(&self) -> f32 {
        unsafe {
            let dpi = GetDpiForWindow(self.hwnd);
            if dpi > 0 { dpi as f32 / 96.0 } else { 1.0 }
        }
    }

    fn destination_monitor(&self, caret: Option<ScreenPoint>) -> HMONITOR {
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(DPI_AWARENESS_CONTEXT_PMV2 as _));
            if let Some(caret) = caret {
                return MonitorFromPoint(
                    POINT {
                        x: caret.x,
                        y: caret.y,
                    },
                    MONITOR_DEFAULTTONEAREST,
                );
            }

            let foreground = GetForegroundWindow();
            if !foreground.is_invalid() {
                MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST)
            } else {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
            }
        }
    }

    fn monitor_dpi_scale(&self, monitor: HMONITOR) -> f32 {
        unsafe {
            let mut dpi_x = 0;
            let mut dpi_y = 0;
            if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok()
                && dpi_x > 0
            {
                dpi_x as f32 / 96.0
            } else {
                self.get_dpi_scale()
            }
        }
    }

    fn placement_context(
        &self,
        phys_pt: ScreenPoint,
        window_size: PixelSize,
        monitor: HMONITOR,
        dpi_scale: f32,
    ) -> PlacementContext {
        unsafe {
            let mut mi = MONITORINFO {
                cbSize: mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(monitor, &mut mi);
            PlacementContext {
                caret: phys_pt,
                work_area: work_area_from_rect(mi.rcWork),
                window_size,
                dpi_scale,
                metrics: self.size.metrics(),
            }
        }
    }

    fn target_position(
        &self,
        placement: PlacementPlan,
        caret: Option<ScreenPoint>,
        window_size: PixelSize,
        monitor: HMONITOR,
        dpi_scale: f32,
    ) -> PixelPoint {
        match (placement, caret) {
            (PlacementPlan::AtCaret(anchor), Some(caret)) => place_at_caret(
                self.placement_context(caret, window_size, monitor, dpi_scale),
                anchor,
            ),
            _ => self.center_position(window_size, monitor),
        }
    }

    fn center_position(&self, window_size: PixelSize, monitor: HMONITOR) -> PixelPoint {
        unsafe {
            let mut mi = MONITORINFO {
                cbSize: mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(monitor, &mut mi);
            center_in_work_area(work_area_from_rect(mi.rcWork), window_size)
        }
    }

    fn set_physical_position(&mut self, x: i32, y: i32) {
        self.desired_phys_x = x;
        self.desired_phys_y = y;
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

impl Drop for BubbleWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.hwnd.is_invalid() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

fn work_area_from_rect(rect: RECT) -> WorkArea {
    WorkArea {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

fn create_overlay_window() -> windows::core::Result<HWND> {
    unsafe {
        let hinstance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(bubble_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hbrBackground: HBRUSH::default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED,
            CLASS_NAME,
            w!(""),
            WS_POPUP,
            0,
            0,
            100,
            50,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;

        // DWM composition for hardware transparency
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // Make layered window fully opaque (DWM handles the transparency)
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA).ok();

        Ok(hwnd)
    }
}

unsafe extern "system" fn bubble_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn resolve_dark_mode(theme_mode: ThemeMode) -> bool {
    match theme_mode {
        ThemeMode::System => is_dark_mode(),
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::Custom => false,
    }
}

fn is_dark_mode() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, KEY_READ};

    let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let Ok(Some(key)) = RegistryKey::open_optional(HKEY_CURRENT_USER, subkey, KEY_READ) else {
        return true;
    };
    key.query_u32(w!("AppsUseLightTheme"))
        .ok()
        .flatten()
        .map(|value| value == 0)
        .unwrap_or(true)
}
