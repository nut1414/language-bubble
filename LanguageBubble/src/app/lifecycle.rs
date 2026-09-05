use std::mem;

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use super::dispatch::msg_wnd_proc;
use super::{APP, AppState};
use crate::types::{HookKeyCombo, SwitchMode};
use crate::{bubble, capslock, hook, language, settings, tray, update};

const MSG_WINDOW_CLASS: PCWSTR = w!("LanguageBubbleMsgWindow");
const MUTEX_NAME: PCWSTR = w!("Global\\LanguageBubble_SingleInstance");
const DPI_AWARENESS_CONTEXT_PMV2: isize = -4;

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

pub(crate) fn run() -> Result<()> {
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

pub(crate) fn show_startup_error(error: &Error) {
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
