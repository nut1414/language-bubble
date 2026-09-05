//! UI-thread application ownership and dispatch.
//!
//! The message callback borrows AppState only while handling a message. COM and
//! modal APIs can reenter it: nested switches coalesce into one latest slot and
//! update delivery waits until the outer borrow is released. Unused/no-layout
//! switch exits deliberately leave pending slots alone, matching prior behavior.
//!
//! Startup acquires COM, mutex, message window, bubble/tray, then keyboard hook.
//! Failure drops acquired locals in reverse order. After the message loop,
//! AppState (hook before bubble/tray by field order) is cleared before the message
//! window, mutex, and COM apartment are dropped. Keep that order on extraction.

mod deferred;
mod dispatch;
mod lifecycle;
mod menu;

pub(crate) use lifecycle::{run, show_startup_error};

use std::cell::RefCell;

use crate::types::*;
use crate::{bubble, hook, language, settings, tray};
use deferred::DeferredEvents;
use dispatch::on_update_available;

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

impl AppState {
    fn tray_menu_snapshot(&self) -> tray::TrayMenuSnapshot {
        tray::TrayMenuSnapshot {
            layouts: self.language_service.layouts().to_vec(),
            current_hkl: self
                .language_service
                .get_current_layout()
                .map(|layout| layout.hkl),
            start_with_windows: settings::is_start_with_windows(),
            size: self.bubble.size(),
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
    static DEFERRED: DeferredEvents = const { DeferredEvents::new() };
}

fn with_app<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut AppState) -> R,
{
    APP.with(|cell| DEFERRED.with(|events| events.with_state(cell, f, on_update_available)))
}
