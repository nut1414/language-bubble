#![windows_subsystem = "windows"]

mod animation;
mod bubble;
mod capslock;
mod caret;
mod hook;
mod language;
mod settings;
mod tray;
mod types;

use std::cell::RefCell;
use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::*;

use windows::Win32::UI::HiDpi::*;

use types::*;

const MSG_WINDOW_CLASS: PCWSTR = w!("LanguageBubbleMsgWindow");
const MUTEX_NAME: PCWSTR = w!("Global\\LanguageBubble_SingleInstance");
const WM_SETTINGCHANGE: u32 = 0x001A;
const DPI_AWARENESS_CONTEXT_PMV2: isize = -4;

struct AppState {
    language_service: language::LanguageService,
    bubble: bubble::BubbleWindow,
    _tray: tray::TrayIcon,
    caps_lock_mode: SwitchMode,
    win_space_mode: SwitchMode,
    alt_shift_mode: SwitchMode,
    caps_lock_display: DisplayMode,
    win_space_display: DisplayMode,
    alt_shift_display: DisplayMode,
    hide_on_typing: bool,
    expanded_mru_only: bool,
    is_switching: bool,
    pending_combo: Option<HookKeyCombo>,
}

// Everything runs on the main (message pump) thread, so thread_local RefCell is safe.
thread_local! {
    static APP: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

fn with_app<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut AppState) -> R,
{
    APP.with(|cell| {
        let mut borrow = cell.borrow_mut();
        borrow.as_mut().map(f)
    })
}

fn main() {
    // Declare Per-Monitor DPI Awareness v2 before any window creation.
    // Without this, Windows virtualizes coordinates at 96 DPI and
    // bitmap-stretches the window, causing blurriness at >100% scaling.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT(
            DPI_AWARENESS_CONTEXT_PMV2 as _,
        ));
    }

    // COM initialization for UI Automation
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    // Single-instance check
    let _mutex = unsafe {
        let h = CreateMutexW(None, true, MUTEX_NAME).ok();
        if h.is_none() || windows::Win32::Foundation::GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = MessageBoxW(
                None,
                w!("Language Bubble is already running."),
                w!("Language Bubble"),
                MB_OK | MB_ICONINFORMATION,
            );
            return;
        }
        h
    };

    settings::migrate_old_settings();
    settings::migrate_display_mode_settings();

    let caps_lock_mode = settings::get_key_mode("CapsLockMode", SwitchMode::AllLanguage);
    let win_space_mode = settings::get_key_mode("WinSpaceMode", SwitchMode::Unused);
    let alt_shift_mode = settings::get_key_mode("AltShiftMode", SwitchMode::Unused);
    let caps_lock_display = settings::get_key_display_mode("CapsLockDisplayMode", DisplayMode::Carousel);
    let win_space_display = settings::get_key_display_mode("WinSpaceDisplayMode", DisplayMode::Carousel);
    let alt_shift_display = settings::get_key_display_mode("AltShiftDisplayMode", DisplayMode::Carousel);
    let hide_on_typing = settings::get_hide_on_typing();
    let expanded_mru_only = settings::get_expanded_mru_only();

    // Create message-only window
    let msg_hwnd = create_msg_window();

    // Language service
    let mut language_service = language::LanguageService::new();
    if let Some(initial) = language_service.get_current_layout() {
        let hkl = initial.hkl;
        language_service.record_layout_usage(hkl);
    }

    // Bubble window
    let mut bubble_win = bubble::BubbleWindow::new(msg_hwnd).expect("Failed to create bubble window");
    bubble_win.set_size(settings::get_bubble_size());

    // Extract icon to temp file
    let icon_path = extract_icon();

    // Tray icon
    let tray_icon = tray::TrayIcon::create(msg_hwnd, icon_path.as_deref());

    // Install keyboard hook
    hook::install(msg_hwnd, caps_lock_mode, win_space_mode, alt_shift_mode);

    // Force Caps Lock off on startup if intercepting
    if caps_lock_mode != SwitchMode::Unused {
        capslock::ensure_caps_lock_off();
    }

    // Store app state
    APP.with(|cell| {
        *cell.borrow_mut() = Some(AppState {
            language_service,
            bubble: bubble_win,
            _tray: tray_icon,
            caps_lock_mode,
            win_space_mode,
            alt_shift_mode,
            caps_lock_display,
            win_space_display,
            alt_shift_display,
            hide_on_typing,
            expanded_mru_only,
            is_switching: false,
            pending_combo: None,
        });
    });

    // Message loop
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // Cleanup
    hook::uninstall();
    APP.with(|cell| {
        *cell.borrow_mut() = None;
    });

    // Delete temp icon
    if let Some(ref path) = icon_path {
        let _ = std::fs::remove_file(path);
    }
}

fn create_msg_window() -> HWND {
    unsafe {
        let hinstance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(msg_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: MSG_WINDOW_CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            MSG_WINDOW_CLASS,
            w!(""),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance.into()),
            None,
        )
        .unwrap()
    }
}

unsafe extern "system" fn msg_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
    match msg {
        m if m == hook::WM_SWITCH_KEY => {
            let combo = match wparam.0 {
                0 => HookKeyCombo::CapsLock,
                1 => HookKeyCombo::WinSpace,
                2 => HookKeyCombo::AltShift,
                _ => return LRESULT(0),
            };
            on_switch_key(combo);
            LRESULT(0)
        }
        m if m == hook::WM_ANY_KEY => {
            with_app(|state| {
                if state.hide_on_typing {
                    state.bubble.instant_hide();
                }
            });
            LRESULT(0)
        }
        m if m == tray::WM_TRAY_CALLBACK => {
            let mouse_msg = (lparam.0 & 0xFFFF) as u32;
            if mouse_msg == WM_RBUTTONUP {
                on_tray_right_click(hwnd);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            let timer_id = wparam.0;
            with_app(|state| {
                match timer_id {
                    bubble::TIMER_HIDE => {
                        let _ = KillTimer(Some(hwnd), bubble::TIMER_HIDE);
                        state.bubble.begin_hide();
                    }
                    bubble::TIMER_TOPMOST => {
                        state.bubble.refresh_topmost();
                    }
                    bubble::TIMER_ANIM => {
                        state.bubble.tick();
                    }
                    _ => {}
                }
            });
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            if lparam.0 != 0 {
                let param = PCWSTR(lparam.0 as *const u16);
                if param == w!("ImmersiveColorSet") {
                    with_app(|state| {
                        state.bubble.refresh_theme();
                    });
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
    }
}

fn on_switch_key(combo: HookKeyCombo) {
    with_app(|state| {
        if state.is_switching {
            state.pending_combo = Some(combo);
            return;
        }
        state.is_switching = true;
        process_switch(state, combo);
    });
}

fn process_switch(state: &mut AppState, combo: HookKeyCombo) {
    let mode = match combo {
        HookKeyCombo::CapsLock => state.caps_lock_mode,
        HookKeyCombo::WinSpace => state.win_space_mode,
        HookKeyCombo::AltShift => state.alt_shift_mode,
    };
    if mode == SwitchMode::Unused {
        state.is_switching = false;
        return;
    }

    // Set display mode for this key binding
    state.bubble.display_mode = match combo {
        HookKeyCombo::CapsLock => state.caps_lock_display,
        HookKeyCombo::WinSpace => state.win_space_display,
        HookKeyCombo::AltShift => state.alt_shift_display,
    };

    // Ensure CapsLock stays off
    if combo == HookKeyCombo::CapsLock {
        capslock::ensure_caps_lock_off();
    }

    // Record current layout before switching
    if let Some(before) = state.language_service.get_current_layout() {
        let hkl = before.hkl;
        state.language_service.record_layout_usage(hkl);
    }

    // Switch language
    let new_layout = if mode == SwitchMode::Mru {
        state.language_service.switch_to_mru().cloned()
    } else {
        state.language_service.switch_to_next().cloned()
    };

    let Some(new_layout) = new_layout else {
        state.is_switching = false;
        return;
    };

    state.language_service.record_layout_usage(new_layout.hkl);

    // Get caret position
    let caret_pos = caret::get_caret_screen_position();

    // Pick which layouts to show
    let display_layouts: Vec<language::LayoutInfo> = if mode == SwitchMode::AllLanguage {
        state.language_service.layouts().to_vec()
    } else if state.expanded_mru_only && state.bubble.display_mode == DisplayMode::Expanded {
        state.language_service.get_mru_layouts()
    } else {
        state.language_service.layouts().to_vec()
    };

    let selected_index = display_layouts
        .iter()
        .position(|l| l.hkl == new_layout.hkl)
        .unwrap_or(0) as i32;

    state
        .bubble
        .show_bubble(&display_layouts, selected_index, caret_pos);

    // Process pending
    let pending = state.pending_combo.take();
    if let Some(next_combo) = pending {
        process_switch(state, next_combo);
    } else {
        state.is_switching = false;
    }
}

fn on_tray_right_click(hwnd: HWND) {
    let data = with_app(|state| {
        (
            state.language_service.layouts().to_vec(),
            state.language_service.get_current_layout().map(|l| l.hkl),
            settings::is_start_with_windows(),
            state.bubble.size,
            state.caps_lock_mode,
            state.win_space_mode,
            state.alt_shift_mode,
            state.caps_lock_display,
            state.win_space_display,
            state.alt_shift_display,
            state.hide_on_typing,
            state.expanded_mru_only,
        )
    });

    let Some((layouts, current_hkl, start_with_windows, size, caps, winsp, altsh, caps_d, winsp_d, altsh_d, hot, emru)) = data else {
        return;
    };

    let Some(cmd) = tray::show_context_menu(
        hwnd,
        &layouts,
        current_hkl,
        start_with_windows,
        size,
        caps,
        winsp,
        altsh,
        caps_d,
        winsp_d,
        altsh_d,
        hot,
        emru,
    ) else {
        return;
    };

    handle_menu_command(cmd);
}

fn handle_menu_command(cmd: u16) {
    with_app(|state| {
        match cmd {
            tray::CMD_EXIT => {
                unsafe { PostQuitMessage(0) };
            }
            tray::CMD_START_WITH_WINDOWS => {
                let current = settings::is_start_with_windows();
                settings::set_start_with_windows(!current);
            }
            tray::CMD_HIDE_ON_TYPING => {
                state.hide_on_typing = !state.hide_on_typing;
                settings::save_hide_on_typing(state.hide_on_typing);
            }
            tray::CMD_EXPANDED_MRU_ONLY => {
                state.expanded_mru_only = !state.expanded_mru_only;
                settings::save_expanded_mru_only(state.expanded_mru_only);
            }
            c if c >= tray::CMD_SIZE_BASE && c < tray::CMD_SIZE_BASE + 5 => {
                let sizes = [
                    BubbleSize::ExtraSmall,
                    BubbleSize::Small,
                    BubbleSize::Medium,
                    BubbleSize::Large,
                    BubbleSize::ExtraLarge,
                ];
                let size = sizes[(c - tray::CMD_SIZE_BASE) as usize];
                state.bubble.set_size(size);
                settings::save_bubble_size(size);
            }
            c if c >= tray::CMD_KEY_CAPSLOCK_BASE && c < tray::CMD_KEY_CAPSLOCK_BASE + 3 => {
                let mode = key_mode_from_index((c - tray::CMD_KEY_CAPSLOCK_BASE) as usize);
                state.caps_lock_mode = mode;
                hook::set_caps_lock_mode(mode);
                settings::save_key_mode("CapsLockMode", mode);
                if mode != SwitchMode::Unused {
                    capslock::ensure_caps_lock_off();
                }
            }
            c if c >= tray::CMD_KEY_WINSPACE_BASE && c < tray::CMD_KEY_WINSPACE_BASE + 3 => {
                let mode = key_mode_from_index((c - tray::CMD_KEY_WINSPACE_BASE) as usize);
                state.win_space_mode = mode;
                hook::set_win_space_mode(mode);
                settings::save_key_mode("WinSpaceMode", mode);
            }
            c if c >= tray::CMD_KEY_ALTSHIFT_BASE && c < tray::CMD_KEY_ALTSHIFT_BASE + 3 => {
                let mode = key_mode_from_index((c - tray::CMD_KEY_ALTSHIFT_BASE) as usize);
                state.alt_shift_mode = mode;
                hook::set_alt_shift_mode(mode);
                settings::save_key_mode("AltShiftMode", mode);
            }
            c if c >= tray::CMD_KEY_CAPSLOCK_DISPLAY_BASE && c < tray::CMD_KEY_CAPSLOCK_DISPLAY_BASE + 3 => {
                let mode = display_mode_from_index((c - tray::CMD_KEY_CAPSLOCK_DISPLAY_BASE) as usize);
                state.caps_lock_display = mode;
                settings::save_key_display_mode("CapsLockDisplayMode", mode);
            }
            c if c >= tray::CMD_KEY_WINSPACE_DISPLAY_BASE && c < tray::CMD_KEY_WINSPACE_DISPLAY_BASE + 3 => {
                let mode = display_mode_from_index((c - tray::CMD_KEY_WINSPACE_DISPLAY_BASE) as usize);
                state.win_space_display = mode;
                settings::save_key_display_mode("WinSpaceDisplayMode", mode);
            }
            c if c >= tray::CMD_KEY_ALTSHIFT_DISPLAY_BASE && c < tray::CMD_KEY_ALTSHIFT_DISPLAY_BASE + 3 => {
                let mode = display_mode_from_index((c - tray::CMD_KEY_ALTSHIFT_DISPLAY_BASE) as usize);
                state.alt_shift_display = mode;
                settings::save_key_display_mode("AltShiftDisplayMode", mode);
            }
            _ => {}
        }
    });
}

fn key_mode_from_index(i: usize) -> SwitchMode {
    match i {
        0 => SwitchMode::AllLanguage,
        1 => SwitchMode::Mru,
        _ => SwitchMode::Unused,
    }
}

fn display_mode_from_index(i: usize) -> DisplayMode {
    match i {
        0 => DisplayMode::Carousel,
        1 => DisplayMode::Simple,
        _ => DisplayMode::Expanded,
    }
}

fn extract_icon() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let ico_path = exe.with_extension("ico");
    if ico_path.exists() {
        return Some(ico_path.to_string_lossy().to_string());
    }
    let res_ico = exe.parent()?.join("resources").join("app.ico");
    if res_ico.exists() {
        return Some(res_ico.to_string_lossy().to_string());
    }
    None
}
