#![windows_subsystem = "windows"]

mod animation;
mod bubble;
mod bubble_layout;
mod capslock;
mod caret;
mod hook;
mod language;
mod registry;
mod settings;
mod tray;
mod types;
mod update;

use std::cell::{Cell, RefCell};
use std::mem;

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use windows::Win32::UI::HiDpi::*;

use types::*;

const MSG_WINDOW_CLASS: PCWSTR = w!("LanguageBubbleMsgWindow");
const MUTEX_NAME: PCWSTR = w!("Global\\LanguageBubble_SingleInstance");
const WM_SETTINGCHANGE: u32 = 0x001A;
const DPI_AWARENESS_CONTEXT_PMV2: isize = -4;

struct AppState {
    hook: hook::InstalledHook,
    settings: settings::UserSettingsStore,
    language_service: language::LanguageService,
    bubble: bubble::BubbleWindow,
    _tray: tray::TrayIcon,
    bindings: KeyBindings,
    hide_on_typing: bool,
    expanded_mru_only: bool,
    theme_mode: ThemeMode,
    custom_colors: CustomThemeColors,
    is_switching: bool,
    pending_combo: Option<HookKeyCombo>,
    pending_update: Option<String>,
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct OwnedWindow(HWND);

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_invalid() {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

impl AppState {
    fn tray_menu_snapshot(&self) -> tray::TrayMenuSnapshot {
        tray::TrayMenuSnapshot {
            layouts: self.language_service.layouts().to_vec(),
            current_hkl: self
                .language_service
                .get_current_layout()
                .map(|layout| layout.hkl),
            start_with_windows: settings::is_start_with_windows(),
            size: self.bubble.size,
            bindings: self.bindings,
            hide_on_typing: self.hide_on_typing,
            expanded_mru_only: self.expanded_mru_only,
            theme_mode: self.theme_mode,
            custom_colors: self.custom_colors,
            check_for_updates: self.settings.check_for_updates(),
            pending_update: self.pending_update.clone(),
            app_version: env!("CARGO_PKG_VERSION"),
            is_msix: settings::is_msix_packaged(),
        }
    }
}

// Everything runs on the main (message pump) thread, so thread_local RefCell is safe.
thread_local! {
    static APP: RefCell<Option<AppState>> = const { RefCell::new(None) };
    static DEFERRED_SWITCH: Cell<Option<HookKeyCombo>> = const { Cell::new(None) };
}

fn with_app<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut AppState) -> R,
{
    APP.with(|cell| {
        // COM and modal Win32 calls can pump messages and re-enter the window
        // procedure. Never panic across that FFI boundary on a nested borrow.
        let mut borrow = cell.try_borrow_mut().ok()?;
        borrow.as_mut().map(f)
    })
}

fn main() {
    if let Err(error) = run() {
        show_startup_error(&error);
    }
}

fn run() -> Result<()> {
    // Declare Per-Monitor DPI Awareness v2 before any window creation.
    // Without this, Windows virtualizes coordinates at 96 DPI and
    // bitmap-stretches the window, causing blurriness at >100% scaling.
    unsafe {
        let _ =
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT(DPI_AWARENESS_CONTEXT_PMV2 as _));
    }

    // COM initialization for UI Automation
    let _com_apartment = ComApartment::initialize()?;

    // Single-instance check
    let (mutex, already_running) = unsafe {
        let handle = CreateMutexW(None, true, MUTEX_NAME)?;
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        (OwnedHandle(handle), already_running)
    };
    let _mutex = mutex;
    if already_running {
        unsafe {
            let _ = MessageBoxW(
                None,
                w!("Language Bubble is already running."),
                w!("Language Bubble"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        return Ok(());
    }

    let settings_store = settings::UserSettingsStore::registry();
    settings::report_result(
        "migrate old settings",
        settings_store.migrate_old_settings(),
    );
    settings::report_result(
        "migrate display settings",
        settings_store.migrate_display_mode_settings(),
    );

    let bindings = settings_store.load_key_bindings();
    let hide_on_typing = settings_store.hide_on_typing();
    let expanded_mru_only = settings_store.expanded_mru_only();
    let theme_mode = settings_store.theme_mode();
    let custom_colors = settings_store.custom_theme_colors();

    // Restore pending update from registry (if any)
    let pending_update = update::pending_from_registry(settings_store);

    // Create message-only window
    let msg_window = OwnedWindow(create_msg_window()?);
    let msg_hwnd = msg_window.0;

    // Language service
    let mut language_service = language::LanguageService::new();
    if let Some(initial) = language_service.get_current_layout() {
        let hkl = initial.hkl;
        language_service.record_layout_usage(hkl);
    }

    // Bubble window
    let mut bubble_win = bubble::BubbleWindow::new(msg_hwnd)?;
    bubble_win.set_size(settings_store.bubble_size());
    bubble_win.set_theme_mode(theme_mode);
    bubble_win.set_custom_colors(custom_colors);

    // Tray icon
    let tray_icon = tray::TrayIcon::create(msg_hwnd);
    tray_icon.show_balloon(
        "Language Bubble",
        "Running in the system tray. Right-click the tray icon to configure.",
    );

    // Check for updates (non-MSIX only)
    if !settings::is_msix_packaged() && settings_store.check_for_updates() {
        let last_check = settings_store.last_update_check();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now.saturating_sub(last_check) > 24 * 3600 {
            update::check_in_background(msg_hwnd, settings_store);
        }
    }

    // Install keyboard hook
    let installed_hook = hook::InstalledHook::install(msg_hwnd, &bindings)?;

    // Force Caps Lock off on startup if intercepting
    if bindings.get(HookKeyCombo::CapsLock).switch_mode != SwitchMode::Unused {
        capslock::ensure_caps_lock_off();
    }

    // Store app state
    APP.with(|cell| {
        *cell.borrow_mut() = Some(AppState {
            hook: installed_hook,
            settings: settings_store,
            language_service,
            bubble: bubble_win,
            _tray: tray_icon,
            bindings,
            hide_on_typing,
            expanded_mru_only,
            theme_mode,
            custom_colors,
            is_switching: false,
            pending_combo: None,
            pending_update,
        });
    });

    // Message loop
    let message_loop_result = unsafe {
        let mut msg = MSG::default();
        loop {
            let status = GetMessageW(&mut msg, None, 0, 0).0;
            if status == -1 {
                break Err(Error::from_win32());
            }
            if status == 0 {
                break Ok(());
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    };

    // Cleanup
    APP.with(|cell| {
        *cell.borrow_mut() = None;
    });
    message_loop_result
}

fn show_startup_error(error: &Error) {
    let message = format!("Language Bubble could not start.\n\n{error}");
    let wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(wide.as_ptr()),
            w!("Language Bubble"),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn create_msg_window() -> Result<HWND> {
    unsafe {
        let hinstance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
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
                let Ok(combo) = HookKeyCombo::try_from(wparam.0) else {
                    return LRESULT(0);
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
            m if m == update::WM_UPDATE_AVAILABLE => {
                let handled = with_app(|state| {
                    let new_version = {
                        if let Ok(mut guard) = update::PENDING_UPDATE.lock() {
                            guard.take()
                        } else {
                            None
                        }
                    };
                    let Some(version) = new_version else {
                        return;
                    };
                    state.pending_update = Some(version.clone());
                    state
                        ._tray
                        .show_balloon("Language Bubble", &format!("Update available: {}", version));
                });
                if handled.is_none() {
                    // A COM call may have re-entered the window procedure while
                    // AppState is mutably borrowed. Leave the pending value in
                    // place and defer delivery until the outer call unwinds.
                    let _ = PostMessageW(
                        Some(hwnd),
                        update::WM_UPDATE_AVAILABLE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
                LRESULT(0)
            }
            WM_TIMER => {
                let timer_id = wparam.0;
                with_app(|state| match timer_id {
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
                });
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                if is_system_theme_change(lparam) {
                    with_app(|state| {
                        state.bubble.refresh_theme();
                    });
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

/// Returns whether `lparam` names the Windows immersive color setting.
///
/// # Safety
///
/// For `WM_SETTINGCHANGE`, Windows guarantees that a non-null `lparam` points
/// to a null-terminated UTF-16 string for the duration of the callback.
unsafe fn is_system_theme_change(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }

    let setting_name = PCWSTR(lparam.0 as *const u16);
    unsafe {
        setting_name
            .to_string()
            .is_ok_and(|name| name == "ImmersiveColorSet")
    }
}

fn on_switch_key(combo: HookKeyCombo) {
    let handled = with_app(|state| {
        if state.is_switching {
            state.pending_combo = Some(combo);
            return;
        }
        state.is_switching = true;
        process_switch(state, combo);
    });
    if handled.is_none() {
        DEFERRED_SWITCH.with(|pending| pending.set(Some(combo)));
    }
}

fn process_switch(state: &mut AppState, combo: HookKeyCombo) {
    let binding = state.bindings.get(combo);
    let mode = binding.switch_mode;
    if mode == SwitchMode::Unused {
        state.is_switching = false;
        return;
    }

    // Input methods can be added or removed while the utility is running.
    state.language_service.refresh_layouts();

    // Set display mode for this key binding
    state.bubble.display_mode = binding.display_mode;

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
    let pending = state
        .pending_combo
        .take()
        .or_else(|| DEFERRED_SWITCH.with(Cell::take));
    if let Some(next_combo) = pending {
        process_switch(state, next_combo);
    } else {
        state.is_switching = false;
    }
}

fn on_tray_right_click(hwnd: HWND) {
    let Some(snapshot) = with_app(|state| state.tray_menu_snapshot()) else {
        return;
    };
    let Some(cmd) = tray::show_context_menu(hwnd, &snapshot) else {
        return;
    };

    handle_menu_command(hwnd, cmd);
}

fn handle_menu_command(hwnd: HWND, cmd: tray::TrayCommand) {
    match cmd {
        tray::TrayCommand::PickCustomBackground => {
            let initial = with_app(|state| state.custom_colors.bg_color).unwrap_or(0);
            if let Some(new_color) = pick_color(hwnd, initial) {
                with_app(|state| {
                    state.custom_colors.bg_color = new_color;
                    state.bubble.set_custom_colors(state.custom_colors);
                    settings::report_result(
                        "save custom theme colors",
                        state
                            .settings
                            .save_custom_theme_colors(&state.custom_colors),
                    );
                });
            }
            return;
        }
        tray::TrayCommand::PickCustomForeground => {
            let initial = with_app(|state| state.custom_colors.fg_color).unwrap_or(0x00FFFFFF);
            if let Some(new_color) = pick_color(hwnd, initial) {
                with_app(|state| {
                    state.custom_colors.fg_color = new_color;
                    state.bubble.set_custom_colors(state.custom_colors);
                    settings::report_result(
                        "save custom theme colors",
                        state
                            .settings
                            .save_custom_theme_colors(&state.custom_colors),
                    );
                });
            }
            return;
        }
        _ => {}
    }

    with_app(|state| match cmd {
        tray::TrayCommand::Exit => {
            unsafe { PostQuitMessage(0) };
        }
        tray::TrayCommand::Feedback => unsafe {
            let _ = windows::Win32::UI::Shell::ShellExecuteW(
                None,
                w!("open"),
                w!("https://github.com/nut1414/language-bubble/issues"),
                None,
                None,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        },
        tray::TrayCommand::ToggleUpdateChecks => {
            let current = state.settings.check_for_updates();
            settings::report_result(
                "save update preference",
                state.settings.save_check_for_updates(!current),
            );
        }
        tray::TrayCommand::DownloadUpdate => unsafe {
            let _ = windows::Win32::UI::Shell::ShellExecuteW(
                None,
                w!("open"),
                w!("https://github.com/nut1414/language-bubble/releases/latest"),
                None,
                None,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        },
        tray::TrayCommand::ToggleStartWithWindows => {
            let current = settings::is_start_with_windows();
            settings::set_start_with_windows(!current);
        }
        tray::TrayCommand::ToggleHideOnTyping => {
            state.hide_on_typing = !state.hide_on_typing;
            settings::report_result(
                "save hide-on-typing preference",
                state.settings.save_hide_on_typing(state.hide_on_typing),
            );
        }
        tray::TrayCommand::ToggleExpandedMruOnly => {
            state.expanded_mru_only = !state.expanded_mru_only;
            settings::report_result(
                "save expanded MRU preference",
                state
                    .settings
                    .save_expanded_mru_only(state.expanded_mru_only),
            );
        }
        tray::TrayCommand::SetSize(size) => {
            state.bubble.set_size(size);
            settings::report_result("save bubble size", state.settings.save_bubble_size(size));
        }
        tray::TrayCommand::SetSwitchMode { combo, mode } => {
            state.bindings.set_switch_mode(combo, mode);
            state.hook.set_mode(combo, mode);
            settings::report_result(
                "save key switch mode",
                state.settings.save_key_switch_mode(combo, mode),
            );
            if combo == HookKeyCombo::CapsLock && mode != SwitchMode::Unused {
                capslock::ensure_caps_lock_off();
            }
        }
        tray::TrayCommand::SetDisplayMode { combo, mode } => {
            state.bindings.set_display_mode(combo, mode);
            settings::report_result(
                "save key display mode",
                state.settings.save_key_display_mode(combo, mode),
            );
        }
        tray::TrayCommand::SetTheme(mode) => {
            state.theme_mode = mode;
            state.bubble.set_theme_mode(mode);
            settings::report_result("save theme mode", state.settings.save_theme_mode(mode));
        }
        tray::TrayCommand::SetOpacity(opacity) => {
            state.custom_colors.opacity = opacity;
            state.bubble.set_custom_colors(state.custom_colors);
            settings::report_result(
                "save custom theme colors",
                state
                    .settings
                    .save_custom_theme_colors(&state.custom_colors),
            );
        }
        tray::TrayCommand::PickCustomBackground | tray::TrayCommand::PickCustomForeground => {
            unreachable!()
        }
    });
}

fn pick_color(hwnd: HWND, initial: u32) -> Option<u32> {
    use windows::Win32::UI::Controls::Dialogs::*;

    unsafe {
        let mut custom: [COLORREF; 16] = [COLORREF(0); 16];
        let mut cc = CHOOSECOLORW {
            lStructSize: mem::size_of::<CHOOSECOLORW>() as u32,
            hwndOwner: hwnd,
            rgbResult: COLORREF(initial),
            lpCustColors: custom.as_mut_ptr(),
            Flags: CC_RGBINIT | CC_FULLOPEN,
            ..Default::default()
        };
        if ChooseColorW(&mut cc).as_bool() {
            Some(cc.rgbResult.0)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_theme_change_compares_setting_contents() {
        let immersive: Vec<u16> = "ImmersiveColorSet"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let unrelated: Vec<u16> = "intl".encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            assert!(is_system_theme_change(LPARAM(immersive.as_ptr() as isize)));
            assert!(!is_system_theme_change(LPARAM(unrelated.as_ptr() as isize)));
            assert!(!is_system_theme_change(LPARAM(0)));
        }
    }
}
