mod policy;
mod transport;

use crate::settings::{SettingsBackend, SettingsStore, report_result};
use policy::{decide_release, is_newer, is_valid_tag};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_USER};

pub const WM_UPDATE_AVAILABLE: u32 = WM_USER + 2;
pub static PENDING_UPDATE: Mutex<Option<String>> = Mutex::new(None);

pub fn check_in_background(hwnd: HWND, settings: crate::settings::UserSettingsStore) {
    let hwnd_value = hwnd.0 as isize;
    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_value as *mut _);
        check_once(
            settings,
            transport::fetch_latest_tag,
            || {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            },
            |tag| {
                if let Ok(mut guard) = PENDING_UPDATE.lock() {
                    *guard = Some(tag);
                    unsafe {
                        let _ = PostMessageW(
                            Some(hwnd),
                            WM_UPDATE_AVAILABLE,
                            windows::Win32::Foundation::WPARAM(0),
                            windows::Win32::Foundation::LPARAM(0),
                        );
                    }
                }
            },
        );
    });
}

/// Preserve operation order: successful fetch, timestamp write, old-version
/// read, optional version write, notification. A write failure is reported but
/// does not cancel subsequent steps. The clock is never read after fetch failure.
fn check_once<B: SettingsBackend>(
    settings: SettingsStore<B>,
    fetch: impl FnOnce() -> Option<String>,
    now: impl FnOnce() -> u64,
    notify: impl FnOnce(String),
) {
    let Some(tag) = fetch() else {
        return;
    };
    report_result(
        "save last update check",
        settings.save_last_update_check(now()),
    );
    let old_last_seen = settings.last_seen_version();
    let decision = decide_release(&tag, env!("CARGO_PKG_VERSION"), &old_last_seen);
    if decision.save_last_seen {
        report_result(
            "save last seen version",
            settings.save_last_seen_version(&tag),
        );
    }
    if decision.notify {
        notify(tag);
    }
}

/// On startup, restore the "Download update..." menu entry if the registry says
/// we previously saw a release newer than what's currently installed.
pub fn pending_from_registry(settings: crate::settings::UserSettingsStore) -> Option<String> {
    let current = env!("CARGO_PKG_VERSION");
    let last_seen = settings.last_seen_version();
    if last_seen.is_empty() || !is_valid_tag(&last_seen) {
        return None;
    }
    if is_newer(&last_seen, current) {
        Some(last_seen)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use windows::core::{Error, HRESULT, Result};

    #[derive(Clone)]
    struct RecordingBackend {
        events: Rc<RefCell<Vec<String>>>,
        last_seen: String,
        fail_writes: bool,
    }
    impl SettingsBackend for RecordingBackend {
        fn read_string(&self, key: &str) -> Result<Option<String>> {
            self.events.borrow_mut().push(format!("read {key}"));
            Ok(Some(self.last_seen.clone()))
        }
        fn write_string(&self, key: &str, value: &str) -> Result<()> {
            self.events
                .borrow_mut()
                .push(format!("write {key}={value}"));
            if self.fail_writes {
                Err(Error::from_hresult(HRESULT(0x80004005u32 as i32)))
            } else {
                Ok(())
            }
        }
        fn delete_value(&self, _: &str) -> Result<()> {
            panic!("update must not delete settings")
        }
    }

    #[test]
    fn successful_check_preserves_effect_order_even_when_writes_fail() {
        for fail_writes in [false, true] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let backend = RecordingBackend {
                events: events.clone(),
                last_seen: "0.0.1".into(),
                fail_writes,
            };
            check_once(
                SettingsStore::new(backend),
                || {
                    events.borrow_mut().push("fetch".into());
                    Some("999.0.0".into())
                },
                || {
                    events.borrow_mut().push("clock".into());
                    42
                },
                |tag| {
                    events.borrow_mut().push(format!("notify {tag}"));
                },
            );
            assert_eq!(
                *events.borrow(),
                [
                    "fetch",
                    "clock",
                    "write LastUpdateCheck=42",
                    "read LastSeenVersion",
                    "write LastSeenVersion=999.0.0",
                    "notify 999.0.0"
                ]
            );
        }
    }

    #[test]
    fn failed_fetch_has_no_clock_persistence_or_notification_effects() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let backend = RecordingBackend {
            events: events.clone(),
            last_seen: String::new(),
            fail_writes: false,
        };
        check_once(
            SettingsStore::new(backend),
            || None,
            || panic!("clock"),
            |_| panic!("notify"),
        );
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn already_seen_release_only_updates_check_timestamp() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let backend = RecordingBackend {
            events: events.clone(),
            last_seen: "999.0.0".into(),
            fail_writes: false,
        };
        check_once(
            SettingsStore::new(backend),
            || Some("999.0.0".into()),
            || 42,
            |_| panic!("notify"),
        );
        assert_eq!(
            *events.borrow(),
            ["write LastUpdateCheck=42", "read LastSeenVersion"]
        );
    }
}
