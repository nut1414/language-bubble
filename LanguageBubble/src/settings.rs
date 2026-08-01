use windows::ApplicationModel::{Package, StartupTask, StartupTaskState};
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use windows::core::{Error, HRESULT, PCWSTR, Result, w};

use crate::registry::RegistryKey;
use crate::types::*;

const SUBKEY: PCWSTR = w!("Software\\LanguageBubble");
const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP_NAME: PCWSTR = w!("LanguageBubble");
const STARTUP_TASK_ID: &str = "LanguageBubbleStartup";
const GENERIC_FAILURE: HRESULT = HRESULT(0x80004005u32 as i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyBindingSettings {
    switch_key: &'static str,
    display_key: &'static str,
    default_switch_mode: SwitchMode,
    default_display_mode: DisplayMode,
}

const fn key_binding_settings(combo: HookKeyCombo) -> KeyBindingSettings {
    match combo {
        HookKeyCombo::CapsLock => KeyBindingSettings {
            switch_key: "CapsLockMode",
            display_key: "CapsLockDisplayMode",
            default_switch_mode: SwitchMode::AllLanguage,
            default_display_mode: DisplayMode::Carousel,
        },
        HookKeyCombo::WinSpace => KeyBindingSettings {
            switch_key: "WinSpaceMode",
            display_key: "WinSpaceDisplayMode",
            default_switch_mode: SwitchMode::Unused,
            default_display_mode: DisplayMode::Carousel,
        },
        HookKeyCombo::AltShift => KeyBindingSettings {
            switch_key: "AltShiftMode",
            display_key: "AltShiftDisplayMode",
            default_switch_mode: SwitchMode::Unused,
            default_display_mode: DisplayMode::Carousel,
        },
    }
}

pub trait SettingsBackend: Clone {
    fn read_string(&self, key: &str) -> Result<Option<String>>;
    fn write_string(&self, key: &str, value: &str) -> Result<()>;
    fn delete_value(&self, key: &str) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RegistrySettingsBackend;

impl SettingsBackend for RegistrySettingsBackend {
    fn read_string(&self, key: &str) -> Result<Option<String>> {
        let Some(registry_key) = RegistryKey::open_optional(HKEY_CURRENT_USER, SUBKEY, KEY_READ)?
        else {
            return Ok(None);
        };
        let wide = wide_string(key);
        registry_key.query_string(PCWSTR(wide.as_ptr()), 256)
    }

    fn write_string(&self, key: &str, value: &str) -> Result<()> {
        let registry_key = RegistryKey::create(HKEY_CURRENT_USER, SUBKEY)?;
        let wide = wide_string(key);
        registry_key.set_string(PCWSTR(wide.as_ptr()), value)
    }

    fn delete_value(&self, key: &str) -> Result<()> {
        let Some(registry_key) = RegistryKey::open_optional(HKEY_CURRENT_USER, SUBKEY, KEY_WRITE)?
        else {
            return Ok(());
        };
        let wide = wide_string(key);
        registry_key.delete_value(PCWSTR(wide.as_ptr()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SettingsStore<B: SettingsBackend> {
    backend: B,
}

pub type UserSettingsStore = SettingsStore<RegistrySettingsBackend>;

impl<B: SettingsBackend> SettingsStore<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    fn read_string(&self, key: &str) -> Option<String> {
        match self.backend.read_string(key) {
            Ok(value) => value,
            Err(error) => {
                debug_error(&format!("read {key}"), &error);
                None
            }
        }
    }

    pub fn load_key_bindings(&self) -> KeyBindings {
        let mut bindings = KeyBindings::default();
        for combo in HookKeyCombo::ALL {
            let descriptor = key_binding_settings(combo);
            let switch_mode = self
                .read_string(descriptor.switch_key)
                .map(|value| SwitchMode::from_str(&value))
                .unwrap_or(descriptor.default_switch_mode);
            let display_mode = self
                .read_string(descriptor.display_key)
                .map(|value| DisplayMode::from_str(&value))
                .unwrap_or(descriptor.default_display_mode);
            bindings.set_switch_mode(combo, switch_mode);
            bindings.set_display_mode(combo, display_mode);
        }
        bindings
    }

    pub fn save_key_switch_mode(&self, combo: HookKeyCombo, mode: SwitchMode) -> Result<()> {
        self.backend
            .write_string(key_binding_settings(combo).switch_key, mode.as_str())
    }

    pub fn save_key_display_mode(&self, combo: HookKeyCombo, mode: DisplayMode) -> Result<()> {
        self.backend
            .write_string(key_binding_settings(combo).display_key, mode.as_str())
    }

    pub fn bubble_size(&self) -> BubbleSize {
        self.read_string("Size")
            .map(|value| BubbleSize::from_str(&value))
            .unwrap_or(BubbleSize::Medium)
    }

    pub fn save_bubble_size(&self, size: BubbleSize) -> Result<()> {
        self.backend.write_string("Size", size.as_str())
    }

    pub fn bubble_label(&self, primary_lang_id: u16) -> Option<String> {
        self.read_string(&bubble_label_value_name(primary_lang_id))
    }

    pub fn save_bubble_label(&self, primary_lang_id: u16, label: &str) -> Result<()> {
        self.backend
            .write_string(&bubble_label_value_name(primary_lang_id), label)
    }

    pub fn reset_bubble_label(&self, primary_lang_id: u16) -> Result<()> {
        self.backend
            .delete_value(&bubble_label_value_name(primary_lang_id))
    }

    pub fn hide_on_typing(&self) -> bool {
        self.read_string("HideOnTyping").as_deref() == Some("True")
    }

    pub fn save_hide_on_typing(&self, enabled: bool) -> Result<()> {
        self.backend
            .write_string("HideOnTyping", bool_string(enabled))
    }

    pub fn expanded_mru_only(&self) -> bool {
        self.read_string("ExpandedMruOnly").as_deref() == Some("True")
    }

    pub fn save_expanded_mru_only(&self, enabled: bool) -> Result<()> {
        self.backend
            .write_string("ExpandedMruOnly", bool_string(enabled))
    }

    pub fn theme_mode(&self) -> ThemeMode {
        self.read_string("ThemeMode")
            .map(|value| ThemeMode::from_str(&value))
            .unwrap_or(ThemeMode::System)
    }

    pub fn save_theme_mode(&self, mode: ThemeMode) -> Result<()> {
        self.backend.write_string("ThemeMode", mode.as_str())
    }

    pub fn custom_theme_colors(&self) -> CustomThemeColors {
        let mut colors = CustomThemeColors::default();
        if let Some(value) = self.read_string("CustomBG")
            && let Ok(parsed) = u32::from_str_radix(&value, 16)
        {
            colors.bg_color = parsed;
        }
        if let Some(value) = self.read_string("CustomFG")
            && let Ok(parsed) = u32::from_str_radix(&value, 16)
        {
            colors.fg_color = parsed;
        }
        if let Some(value) = self.read_string("CustomOpacity")
            && let Ok(parsed) = value.parse::<u8>()
        {
            colors.opacity = parsed;
        }
        colors
    }

    pub fn save_custom_theme_colors(&self, colors: &CustomThemeColors) -> Result<()> {
        self.backend
            .write_string("CustomBG", &format!("{:06X}", colors.bg_color & 0x00FFFFFF))?;
        self.backend
            .write_string("CustomFG", &format!("{:06X}", colors.fg_color & 0x00FFFFFF))?;
        self.backend
            .write_string("CustomOpacity", &colors.opacity.to_string())
    }

    pub fn check_for_updates(&self) -> bool {
        self.read_string("CheckForUpdates").as_deref() != Some("False")
    }

    pub fn save_check_for_updates(&self, enabled: bool) -> Result<()> {
        self.backend
            .write_string("CheckForUpdates", bool_string(enabled))
    }

    pub fn last_update_check(&self) -> u64 {
        self.read_string("LastUpdateCheck")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    }

    pub fn save_last_update_check(&self, timestamp: u64) -> Result<()> {
        self.backend
            .write_string("LastUpdateCheck", &timestamp.to_string())
    }

    pub fn last_seen_version(&self) -> String {
        self.read_string("LastSeenVersion").unwrap_or_default()
    }

    pub fn save_last_seen_version(&self, version: &str) -> Result<()> {
        self.backend.write_string("LastSeenVersion", version)
    }

    pub fn migrate_old_settings(&self) -> Result<()> {
        if self.backend.read_string("CapsLockMode")?.is_none() {
            let mode = if self.backend.read_string("MruSwitching")?.as_deref() == Some("True") {
                SwitchMode::Mru
            } else {
                SwitchMode::AllLanguage
            };
            self.backend.write_string("CapsLockMode", mode.as_str())?;
        }

        self.backend.delete_value("MruSwitching")
    }

    pub fn migrate_display_mode_settings(&self) -> Result<()> {
        const DISPLAY_MODE_KEYS: [&str; 3] = [
            "CapsLockDisplayMode",
            "WinSpaceDisplayMode",
            "AltShiftDisplayMode",
        ];

        let mut missing_keys = Vec::new();
        for key in DISPLAY_MODE_KEYS {
            if self.backend.read_string(key)?.is_none() {
                missing_keys.push(key);
            }
        }
        if missing_keys.is_empty() {
            return Ok(());
        }

        let mode = self
            .backend
            .read_string("DisplayMode")?
            .map(|value| DisplayMode::from_str(&value))
            .unwrap_or(DisplayMode::Carousel);
        for key in missing_keys {
            self.backend.write_string(key, mode.as_str())?;
        }
        Ok(())
    }
}

impl SettingsStore<RegistrySettingsBackend> {
    pub fn registry() -> Self {
        Self::new(RegistrySettingsBackend)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupRoute {
    Msix,
    Registry,
}

const fn startup_route(packaged: bool) -> StartupRoute {
    if packaged {
        StartupRoute::Msix
    } else {
        StartupRoute::Registry
    }
}

pub fn is_msix_packaged() -> bool {
    Package::Current().is_ok()
}

pub fn is_start_with_windows() -> bool {
    let result = match startup_route(is_msix_packaged()) {
        StartupRoute::Msix => is_start_with_windows_msix(),
        StartupRoute::Registry => is_start_with_windows_registry(),
    };
    match result {
        Ok(enabled) => enabled,
        Err(error) => {
            debug_error("read startup registration", &error);
            false
        }
    }
}

pub fn set_start_with_windows(enable: bool) {
    let result = match startup_route(is_msix_packaged()) {
        StartupRoute::Msix => set_start_with_windows_msix(enable),
        StartupRoute::Registry => set_start_with_windows_registry(enable),
    };
    report_result("write startup registration", result);
}

fn is_start_with_windows_msix() -> Result<bool> {
    let task = StartupTask::GetAsync(&STARTUP_TASK_ID.into())?.get()?;
    let state = task.State()?;
    Ok(matches!(
        state,
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy
    ))
}

fn set_start_with_windows_msix(enable: bool) -> Result<()> {
    let task = StartupTask::GetAsync(&STARTUP_TASK_ID.into())?.get()?;
    if enable {
        let _ = task.RequestEnableAsync()?.get()?;
    } else {
        task.Disable()?;
    }
    Ok(())
}

fn is_start_with_windows_registry() -> Result<bool> {
    let Some(key) = RegistryKey::open_optional(HKEY_CURRENT_USER, RUN_SUBKEY, KEY_READ)? else {
        return Ok(false);
    };
    key.value_exists(APP_NAME)
}

fn set_start_with_windows_registry(enable: bool) -> Result<()> {
    if enable {
        let key = RegistryKey::create(HKEY_CURRENT_USER, RUN_SUBKEY)?;
        let executable = std::env::current_exe()
            .map_err(|error| Error::new(GENERIC_FAILURE, error.to_string()))?;
        key.set_string(APP_NAME, &format!("\"{}\"", executable.display()))
    } else if let Some(key) = RegistryKey::open_optional(HKEY_CURRENT_USER, RUN_SUBKEY, KEY_WRITE)?
    {
        key.delete_value(APP_NAME)
    } else {
        Ok(())
    }
}

pub fn report_result(context: &str, result: Result<()>) {
    if let Err(error) = result {
        debug_error(context, &error);
    }
}

fn debug_error(context: &str, error: &Error) {
    let message = format!("[LanguageBubble] {context}: {error}\n");
    let wide = wide_string(&message);
    unsafe {
        OutputDebugStringW(PCWSTR(wide.as_ptr()));
    }
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn bubble_label_value_name(primary_lang_id: u16) -> String {
    format!("BubbleLabel.{:03X}", primary_lang_id & 0x03FF)
}

const fn bool_string(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;

    use super::*;

    #[derive(Clone, Default)]
    struct MemoryBackend {
        values: Rc<RefCell<HashMap<String, String>>>,
        fail_reads: Rc<Cell<bool>>,
        fail_writes: Rc<Cell<bool>>,
    }

    impl MemoryBackend {
        fn value(&self, key: &str) -> Option<String> {
            self.values.borrow().get(key).cloned()
        }
    }

    impl SettingsBackend for MemoryBackend {
        fn read_string(&self, key: &str) -> Result<Option<String>> {
            if self.fail_reads.get() {
                return Err(Error::new(GENERIC_FAILURE, "memory backend failure"));
            }
            Ok(self.value(key))
        }

        fn write_string(&self, key: &str, value: &str) -> Result<()> {
            if self.fail_writes.get() {
                return Err(Error::new(GENERIC_FAILURE, "memory backend failure"));
            }
            self.values
                .borrow_mut()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn delete_value(&self, key: &str) -> Result<()> {
            if self.fail_writes.get() {
                return Err(Error::new(GENERIC_FAILURE, "memory backend failure"));
            }
            self.values.borrow_mut().remove(key);
            Ok(())
        }
    }

    #[test]
    fn key_binding_registry_names_and_defaults_are_stable() {
        assert_eq!(
            key_binding_settings(HookKeyCombo::CapsLock),
            KeyBindingSettings {
                switch_key: "CapsLockMode",
                display_key: "CapsLockDisplayMode",
                default_switch_mode: SwitchMode::AllLanguage,
                default_display_mode: DisplayMode::Carousel,
            }
        );
        assert_eq!(
            key_binding_settings(HookKeyCombo::WinSpace),
            KeyBindingSettings {
                switch_key: "WinSpaceMode",
                display_key: "WinSpaceDisplayMode",
                default_switch_mode: SwitchMode::Unused,
                default_display_mode: DisplayMode::Carousel,
            }
        );
        assert_eq!(
            key_binding_settings(HookKeyCombo::AltShift),
            KeyBindingSettings {
                switch_key: "AltShiftMode",
                display_key: "AltShiftDisplayMode",
                default_switch_mode: SwitchMode::Unused,
                default_display_mode: DisplayMode::Carousel,
            }
        );
    }

    #[test]
    fn missing_and_invalid_values_keep_existing_defaults() {
        let backend = MemoryBackend::default();
        let store = SettingsStore::new(backend.clone());
        assert_eq!(store.bubble_size(), BubbleSize::Medium);
        assert_eq!(store.theme_mode(), ThemeMode::System);
        assert!(store.check_for_updates());
        assert_eq!(store.load_key_bindings(), KeyBindings::default());

        backend.write_string("Size", "invalid").unwrap();
        backend.write_string("ThemeMode", "invalid").unwrap();
        backend.write_string("CapsLockMode", "invalid").unwrap();
        assert_eq!(store.bubble_size(), BubbleSize::Medium);
        assert_eq!(store.theme_mode(), ThemeMode::System);
        assert_eq!(
            store
                .load_key_bindings()
                .get(HookKeyCombo::CapsLock)
                .switch_mode,
            SwitchMode::Unused
        );
    }

    #[test]
    fn typed_saves_preserve_serialized_values() {
        let backend = MemoryBackend::default();
        let store = SettingsStore::new(backend.clone());
        store.save_bubble_size(BubbleSize::Large).unwrap();
        store.save_hide_on_typing(true).unwrap();
        store.save_expanded_mru_only(true).unwrap();
        store.save_theme_mode(ThemeMode::Custom).unwrap();
        store
            .save_custom_theme_colors(&CustomThemeColors {
                bg_color: 0x00112233,
                fg_color: 0x00AABBCC,
                opacity: 191,
            })
            .unwrap();
        store
            .save_key_switch_mode(HookKeyCombo::WinSpace, SwitchMode::Mru)
            .unwrap();
        store
            .save_key_display_mode(HookKeyCombo::AltShift, DisplayMode::Expanded)
            .unwrap();
        store.save_check_for_updates(false).unwrap();
        store.save_last_update_check(42).unwrap();
        store.save_last_seen_version("v1.2.3").unwrap();

        assert_eq!(backend.value("Size").as_deref(), Some("Large"));
        assert_eq!(backend.value("HideOnTyping").as_deref(), Some("True"));
        assert_eq!(backend.value("ExpandedMruOnly").as_deref(), Some("True"));
        assert_eq!(backend.value("ThemeMode").as_deref(), Some("Custom"));
        assert_eq!(backend.value("CustomBG").as_deref(), Some("112233"));
        assert_eq!(backend.value("CustomFG").as_deref(), Some("AABBCC"));
        assert_eq!(backend.value("CustomOpacity").as_deref(), Some("191"));
        assert_eq!(backend.value("WinSpaceMode").as_deref(), Some("MRU"));
        assert_eq!(
            backend.value("AltShiftDisplayMode").as_deref(),
            Some("Expanded")
        );
        assert_eq!(backend.value("CheckForUpdates").as_deref(), Some("False"));
        assert_eq!(backend.value("LastUpdateCheck").as_deref(), Some("42"));
        assert_eq!(backend.value("LastSeenVersion").as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn migrations_work_without_registry_access() {
        let backend = MemoryBackend::default();
        backend.write_string("MruSwitching", "True").unwrap();
        backend.write_string("DisplayMode", "Expanded").unwrap();
        let store = SettingsStore::new(backend.clone());

        store.migrate_old_settings().unwrap();
        store.migrate_display_mode_settings().unwrap();

        assert_eq!(backend.value("CapsLockMode").as_deref(), Some("MRU"));
        assert_eq!(backend.value("MruSwitching"), None);
        for key in [
            "CapsLockDisplayMode",
            "WinSpaceDisplayMode",
            "AltShiftDisplayMode",
        ] {
            assert_eq!(backend.value(key).as_deref(), Some("Expanded"));
        }
    }

    #[test]
    fn display_migration_preserves_existing_values_and_fills_missing_ones() {
        let backend = MemoryBackend::default();
        backend
            .write_string("CapsLockDisplayMode", "Simple")
            .unwrap();
        backend.write_string("DisplayMode", "Expanded").unwrap();
        let store = SettingsStore::new(backend.clone());

        store.migrate_display_mode_settings().unwrap();

        assert_eq!(
            backend.value("CapsLockDisplayMode").as_deref(),
            Some("Simple")
        );
        for key in ["WinSpaceDisplayMode", "AltShiftDisplayMode"] {
            assert_eq!(backend.value(key).as_deref(), Some("Expanded"));
        }
    }

    #[test]
    fn migration_read_failures_do_not_write_defaults() {
        let backend = MemoryBackend::default();
        backend.fail_reads.set(true);
        let store = SettingsStore::new(backend.clone());

        assert!(store.migrate_old_settings().is_err());
        assert!(store.migrate_display_mode_settings().is_err());
        assert!(backend.values.borrow().is_empty());
    }

    #[test]
    fn backend_failures_fall_back_and_remain_reportable() {
        let backend = MemoryBackend::default();
        backend.fail_reads.set(true);
        backend.fail_writes.set(true);
        let store = SettingsStore::new(backend);
        assert_eq!(store.bubble_size(), BubbleSize::Medium);
        assert!(store.save_bubble_size(BubbleSize::Large).is_err());
    }

    #[test]
    fn bubble_labels_use_primary_language_registry_names() {
        let backend = MemoryBackend::default();
        let store = SettingsStore::new(backend.clone());

        store.save_bubble_label(0x0009, "E").unwrap();
        assert_eq!(store.bubble_label(0x0009).as_deref(), Some("E"));
        assert_eq!(backend.value("BubbleLabel.009").as_deref(), Some("E"));

        store.reset_bubble_label(0x0009).unwrap();
        assert_eq!(store.bubble_label(0x0009), None);
    }

    #[test]
    fn bubble_label_registry_names_are_stable_and_language_scoped() {
        assert_eq!(bubble_label_value_name(0x0009), "BubbleLabel.009");
        assert_eq!(bubble_label_value_name(0x0019), "BubbleLabel.019");
        assert_eq!(bubble_label_value_name(0xFFFF), "BubbleLabel.3FF");
    }

    #[test]
    fn bubble_label_write_failures_are_reportable() {
        let backend = MemoryBackend::default();
        backend.fail_writes.set(true);
        let store = SettingsStore::new(backend);

        assert!(store.save_bubble_label(0x0009, "E").is_err());
        assert!(store.reset_bubble_label(0x0009).is_err());
    }

    #[test]
    fn packaged_startup_routes_to_startup_task() {
        assert_eq!(startup_route(true), StartupRoute::Msix);
        assert_eq!(startup_route(false), StartupRoute::Registry);
    }
}
