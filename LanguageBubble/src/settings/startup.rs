use super::{GENERIC_FAILURE, debug_error, report_result};
use crate::registry::RegistryKey;
use windows::ApplicationModel::{Package, StartupTask, StartupTaskState};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use windows::core::{Error, PCWSTR, Result, w};

const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP_NAME: PCWSTR = w!("LanguageBubble");
const STARTUP_TASK_ID: &str = "LanguageBubbleStartup";

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packaged_startup_routes_to_startup_task() {
        assert_eq!(startup_route(true), StartupRoute::Msix);
        assert_eq!(startup_route(false), StartupRoute::Registry);
    }
}
