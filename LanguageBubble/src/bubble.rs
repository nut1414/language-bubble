use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::animation::*;
use crate::caret::ScreenPoint;
use crate::language::LayoutInfo;
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
    desired_phys_x: i32,
    desired_phys_y: i32,
}

impl BubbleWindow {
    pub fn new(msg_hwnd: HWND) -> windows::core::Result<Self> {
        let d2d_factory: ID2D1Factory = unsafe {
            D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?
        };
        let dwrite_factory: IDWriteFactory = unsafe {
            DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?
        };

        let hwnd = create_overlay_window();

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
            desired_phys_x: 0,
            desired_phys_y: 0,
        };
        bw.create_text_format();
        Ok(bw)
    }

    pub fn set_size(&mut self, size: BubbleSize) {
        self.size = size;
        self.create_text_format();
        self.render_target = None; // Force recreate on next render
    }

    pub fn refresh_theme(&mut self) {
        self.dark_mode = is_dark_mode();
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
        // Stop timers
        unsafe {
            let _ = KillTimer(Some(self.msg_hwnd), TIMER_HIDE);
            let _ = KillTimer(Some(self.msg_hwnd), TIMER_ANIM);
        }

        // Update labels
        self.labels = layouts.iter().map(|l| l.bubble_text.clone()).collect();

        let can_slide = self.previous_selected_index >= 0
            && self.previous_selected_index != selected
            && self.labels.len() > 1
            && caret.is_some();

        // Snapshot current label opacities BEFORE changing selected_index,
        // so we capture what's actually on screen, not the new targets.
        if can_slide {
            let current_opacities: Vec<f32> = (0..self.labels.len())
                .map(|i| self.anim.label_opacity(i, self.get_label_target_opacity(i as i32)))
                .collect();
            self.anim.begin_label_transition(current_opacities);
        }

        // Now update the selection
        self.selected_index = selected;
        // Compute window size
        let metrics = self.size.metrics();
        let count = self.labels.len() as f32;
        let (content_w, content_h) = match self.display_mode {
            DisplayMode::Expanded if self.labels.len() > 1 => {
                (count * metrics.item_width, metrics.item_height)
            }
            _ => (metrics.item_width, metrics.item_height),
        };
        // Scale DIP dimensions to physical pixels for SetWindowPos under PMv2
        let dpi_scale = self.get_dpi_scale();
        let win_w = ((content_w + metrics.padding * 2.0 + 1.0) * dpi_scale) as i32;
        let win_h = ((content_h + metrics.padding * 2.0 + 1.0) * dpi_scale) as i32;

        // Resize and position
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                win_w,
                win_h,
                SWP_NOMOVE | SWP_NOACTIVATE,
            )
            .ok();
        }

        // Recreate render target for new size
        self.render_target = None;

        // Position and animate
        if can_slide && self.display_mode == DisplayMode::Expanded && caret.is_some() {
            // EXPANDED SLIDE: compute target without moving the window yet
            let caret_pt = caret.unwrap();
            let target_x = self.compute_expanded_x(caret_pt, selected, win_w);
            let target_y = self.compute_caret_y(caret_pt, win_w, win_h);

            // Start animation from current position to target
            let delta = selected - self.previous_selected_index;
            let dpi_scale = self.get_dpi_scale();
            let offset = (delta as f32 * metrics.item_width * dpi_scale) as i32;
            let nominal_from = target_x + offset;
            self.anim.begin_window_slide(nominal_from, target_x);

            // Place window at the animation's actual start position (not the target!)
            let start_x = self.anim.win_slide_from_x;
            self.desired_phys_x = target_x;
            self.desired_phys_y = target_y;
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd, Some(HWND_TOPMOST),
                    start_x, target_y, 0, 0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            self.anim.set_visible();
        } else if can_slide && self.display_mode == DisplayMode::Carousel {
            // CAROUSEL SLIDE: position window, animate row offset
            if let Some(caret_pt) = caret {
                self.position_at_caret(caret_pt);
            } else {
                self.center_on_screen();
            }
            let from = -self.previous_selected_index as f32 * metrics.item_width;
            let to = -selected as f32 * metrics.item_width;
            self.anim.begin_slide(from, to);
            self.anim.set_visible();
        } else {
            // FIRST SHOW / SIMPLE: position and fade in
            if let Some(caret_pt) = caret {
                if self.display_mode == DisplayMode::Expanded && self.labels.len() > 1 {
                    self.position_at_caret_expanded(caret_pt, selected);
                } else {
                    self.position_at_caret(caret_pt);
                }
            } else {
                self.center_on_screen();
            }
            self.anim.slide_to = -(selected as f32) * metrics.item_width;
            self.anim.slide_from = self.anim.slide_to;
            self.anim.sliding = false;
            self.anim.begin_fade_in();
        }

        // Show window
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }

        self.render();
        self.previous_selected_index = selected;

        // Start timers
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
                    self.hwnd, Some(HWND_TOPMOST),
                    x, self.desired_phys_y, 0, 0,
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
        let Some(rt) = &self.render_target else {
            return;
        };
        let Some(fmt) = &self.text_format else {
            return;
        };

        let metrics = self.size.metrics();
        let opacity = self.anim.opacity();
        let (bg_color, border_color, fg_base) = if self.dark_mode {
            (DARK_BG, DARK_BORDER, D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 })
        } else {
            (LIGHT_BG, LIGHT_BORDER, D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 })
        };

        unsafe {
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            let size = rt.GetSize();

            // Background rounded rect
            let bg_brush = rt
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        a: bg_color.a * opacity,
                        ..bg_color
                    },
                    None,
                )
                .unwrap();
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
            let border_brush = rt
                .CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        a: border_color.a * opacity,
                        ..border_color
                    },
                    None,
                )
                .unwrap();
            rt.DrawRoundedRectangle(&rrect, &border_brush, 0.5, None);

            // Draw labels
            let slide_offset = if self.display_mode == DisplayMode::Carousel && self.labels.len() > 1
            {
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
                let fg_brush = rt.CreateSolidColorBrush(&fg_color, None).unwrap();

                let x = metrics.padding + i as f32 * metrics.item_width + slide_offset;
                let y = metrics.padding;

                let rect = D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + metrics.item_width,
                    bottom: y + metrics.item_height,
                };

                let wide: Vec<u16> = label_text.encode_utf16().collect();
                rt.DrawText(&wide, fmt, &rect, &fg_brush, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
            }

            let _ = rt.EndDraw(None, None);
        }
    }

    /// Get the *target* opacity for a label (what it should settle at).
    fn get_label_target_opacity(&self, index: i32) -> f32 {
        if self.labels.len() <= 1 {
            return 1.0;
        }
        if self.display_mode == DisplayMode::Simple {
            return if index == self.selected_index { 1.0 } else { 0.0 };
        }
        if index == self.selected_index { 1.0 } else { 0.3 }
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

    fn position_at_caret(&mut self, phys_pt: ScreenPoint) {
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
                DPI_AWARENESS_CONTEXT_PMV2 as _,
            ));

            let dpi_scale = self.get_dpi_scale();
            let margin = (10.0 * dpi_scale) as i32;
            let caret_offset = (4.0 * dpi_scale) as i32;

            let mut rect = RECT::default();
            GetWindowRect(self.hwnd, &mut rect).ok();
            let bw = rect.right - rect.left;
            let bh = rect.bottom - rect.top;

            let mut x = phys_pt.x - bw / 2;
            let mut y = phys_pt.y + caret_offset;

            // Clamp to monitor work area
            let pt = POINT {
                x: phys_pt.x,
                y: phys_pt.y,
            };
            let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(hmon, &mut mi);

            if x + bw > mi.rcWork.right - margin {
                x = mi.rcWork.right - bw - margin;
            }
            if x < mi.rcWork.left + margin {
                x = mi.rcWork.left + margin;
            }
            if y + bh > mi.rcWork.bottom - margin {
                y = phys_pt.y - bh - caret_offset; // flip above
            }
            if y < mi.rcWork.top + margin {
                y = mi.rcWork.top + margin;
            }

            self.set_physical_position(x, y);
        }
    }

    fn position_at_caret_expanded(&mut self, phys_pt: ScreenPoint, selected: i32) {
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
                DPI_AWARENESS_CONTEXT_PMV2 as _,
            ));

            let dpi_scale = self.get_dpi_scale();
            let margin = (10.0 * dpi_scale) as i32;
            let caret_offset = (4.0 * dpi_scale) as i32;

            let mut rect = RECT::default();
            GetWindowRect(self.hwnd, &mut rect).ok();
            let bw = rect.right - rect.left;
            let bh = rect.bottom - rect.top;

            let metrics = self.size.metrics();
            let selected_center_dip =
                metrics.padding + selected as f32 * metrics.item_width + metrics.item_width / 2.0;
            let selected_center_phys = (selected_center_dip * dpi_scale) as i32;

            let mut x = phys_pt.x - selected_center_phys;
            let mut y = phys_pt.y + caret_offset;

            // Clamp to monitor
            let pt = POINT {
                x: phys_pt.x,
                y: phys_pt.y,
            };
            let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(hmon, &mut mi);

            if x + bw > mi.rcWork.right - margin {
                x = mi.rcWork.right - bw - margin;
            }
            if x < mi.rcWork.left + margin {
                x = mi.rcWork.left + margin;
            }
            if y + bh > mi.rcWork.bottom - margin {
                y = phys_pt.y - bh - caret_offset;
            }
            if y < mi.rcWork.top + margin {
                y = mi.rcWork.top + margin;
            }

            self.set_physical_position(x, y);
        }
    }

    /// Compute the target X position for expanded mode without moving the window.
    fn compute_expanded_x(&self, phys_pt: ScreenPoint, selected: i32, _win_w: i32) -> i32 {
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
                DPI_AWARENESS_CONTEXT_PMV2 as _,
            ));

            let dpi_scale = self.get_dpi_scale();
            let margin = (10.0 * dpi_scale) as i32;

            let mut rect = RECT::default();
            GetWindowRect(self.hwnd, &mut rect).ok();
            let bw = rect.right - rect.left;

            let metrics = self.size.metrics();
            let selected_center_dip =
                metrics.padding + selected as f32 * metrics.item_width + metrics.item_width / 2.0;
            let selected_center_phys = (selected_center_dip * dpi_scale) as i32;

            let mut x = phys_pt.x - selected_center_phys;

            // Clamp to monitor
            let pt = POINT { x: phys_pt.x, y: phys_pt.y };
            let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(hmon, &mut mi);

            if x + bw > mi.rcWork.right - margin {
                x = mi.rcWork.right - bw - margin;
            }
            if x < mi.rcWork.left + margin {
                x = mi.rcWork.left + margin;
            }
            x
        }
    }

    /// Compute the target Y position for caret-relative placement.
    fn compute_caret_y(&self, phys_pt: ScreenPoint, _win_w: i32, _win_h: i32) -> i32 {
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
                DPI_AWARENESS_CONTEXT_PMV2 as _,
            ));

            let dpi_scale = self.get_dpi_scale();
            let margin = (10.0 * dpi_scale) as i32;
            let caret_offset = (4.0 * dpi_scale) as i32;

            let mut rect = RECT::default();
            GetWindowRect(self.hwnd, &mut rect).ok();
            let bh = rect.bottom - rect.top;

            let mut y = phys_pt.y + caret_offset;

            let pt = POINT { x: phys_pt.x, y: phys_pt.y };
            let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(hmon, &mut mi);

            if y + bh > mi.rcWork.bottom - margin {
                y = phys_pt.y - bh - caret_offset;
            }
            if y < mi.rcWork.top + margin {
                y = mi.rcWork.top + margin;
            }
            y
        }
    }

    fn center_on_screen(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
                DPI_AWARENESS_CONTEXT_PMV2 as _,
            ));

            let mut rect = RECT::default();
            GetWindowRect(self.hwnd, &mut rect).ok();
            let bw = rect.right - rect.left;
            let bh = rect.bottom - rect.top;

            let fg = GetForegroundWindow();
            let hmon = if !fg.is_invalid() {
                MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST)
            } else {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
            };

            let mut mi = MONITORINFO {
                cbSize: mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let _ = GetMonitorInfoW(hmon, &mut mi);

            let work_w = mi.rcWork.right - mi.rcWork.left;
            let work_h = mi.rcWork.bottom - mi.rcWork.top;
            let x = mi.rcWork.left + (work_w - bw) / 2;
            let y = mi.rcWork.top + (work_h - bh) / 2;

            self.set_physical_position(x, y);
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

fn create_overlay_window() -> HWND {
    unsafe {
        let hinstance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .unwrap_or_default();
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
        )
        .unwrap();

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

        hwnd
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

fn is_dark_mode() -> bool {
    use windows::Win32::System::Registry::*;
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, Some(0), KEY_READ, &mut hkey).is_err() {
            return true;
        }
        let mut val: u32 = 1;
        let mut size = mem::size_of::<u32>() as u32;
        let mut kind = windows::Win32::System::Registry::REG_VALUE_TYPE::default();
        let result = RegQueryValueExW(
            hkey,
            w!("AppsUseLightTheme"),
            None,
            Some(&mut kind),
            Some(&mut val as *mut u32 as *mut u8),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        if result.is_ok() {
            val == 0
        } else {
            true
        }
    }
}
