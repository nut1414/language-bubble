mod renderer;
use renderer::{Frame, Renderer};

use std::mem;

use windows::Win32::Foundation::*;
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

pub struct BubbleWindow {
    hwnd: HWND,
    /// The message-only window that handles WM_TIMER dispatching.
    msg_hwnd: HWND,
    renderer: Renderer,
    anim: AnimController,
    size: BubbleSize,
    display_mode: DisplayMode,
    labels: Vec<String>,
    selected_index: i32,
    previous_selected_index: i32,
    dark_mode: bool,
    theme_mode: ThemeMode,
    custom_colors: CustomThemeColors,
    desired_phys_x: i32,
    desired_phys_y: i32,
}

impl BubbleWindow {
    pub fn size(&self) -> BubbleSize {
        self.size
    }
    pub fn display_mode(&self) -> DisplayMode {
        self.display_mode
    }
    pub fn set_display_mode(&mut self, mode: DisplayMode) {
        self.display_mode = mode;
    }

    pub fn new(msg_hwnd: HWND) -> windows::core::Result<Self> {
        let renderer = Renderer::new()?;

        let hwnd = create_overlay_window()?;

        let mut bw = Self {
            hwnd,
            msg_hwnd,
            renderer,
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
        bw.renderer.create_text_format(bw.size);
        Ok(bw)
    }

    pub fn set_size(&mut self, size: BubbleSize) {
        self.size = size;
        self.renderer.create_text_format(self.size);
        self.renderer.rebuild_text_layouts(&self.labels, self.size);
        self.renderer.invalidate();
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

    pub fn show_bubble(
        &mut self,
        layouts: &[LayoutInfo],
        selected: i32,
        caret: Option<ScreenPoint>,
    ) {
        self.stop_show_timers();
        self.labels = layouts.iter().map(|l| l.bubble_text.clone()).collect();
        self.renderer.rebuild_text_layouts(&self.labels, self.size);
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
        self.renderer.invalidate();
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

    fn frame(&self) -> Frame<'_> {
        Frame {
            size: self.size,
            anim: &self.anim,
            theme_mode: self.theme_mode,
            custom_colors: self.custom_colors,
            dark_mode: self.dark_mode,
            display_mode: self.display_mode,
            labels: &self.labels,
            selected_index: self.selected_index,
        }
    }

    fn get_label_target_opacity(&self, index: i32) -> f32 {
        self.frame().get_label_target_opacity(index)
    }

    fn render(&mut self) {
        let frame = Frame {
            size: self.size,
            anim: &self.anim,
            theme_mode: self.theme_mode,
            custom_colors: self.custom_colors,
            dark_mode: self.dark_mode,
            display_mode: self.display_mode,
            labels: &self.labels,
            selected_index: self.selected_index,
        };
        self.renderer.render(self.hwnd, frame);
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
