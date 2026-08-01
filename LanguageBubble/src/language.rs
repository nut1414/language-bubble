use std::collections::HashMap;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_INPUTLANGCHANGEREQUEST: u32 = 0x0050;
const KLF_SETFORPROCESS: ACTIVATE_KEYBOARD_LAYOUT_FLAGS =
    ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0x00000100);

#[derive(Debug, Clone)]
pub struct LayoutInfo {
    pub hkl: HKL,
    pub lang_id: u16,
    pub primary_lang_id: u16,
    pub iso_code: String,
    pub english_name: String,
    pub bubble_text: String,
}

pub struct LanguageService {
    layouts: Vec<LayoutInfo>,
    last_non_english_hkl: Option<HKL>,
    label_overrides: HashMap<u16, Option<String>>,
}

impl LanguageService {
    pub fn new() -> Self {
        let mut svc = Self {
            layouts: Vec::new(),
            last_non_english_hkl: None,
            label_overrides: HashMap::new(),
        };
        svc.refresh_layouts();
        svc
    }

    pub fn refresh_layouts(&mut self) {
        unsafe {
            let count = GetKeyboardLayoutList(None) as usize;
            if count == 0 {
                return;
            }
            let mut hkls = vec![HKL::default(); count];
            let copied = GetKeyboardLayoutList(Some(&mut hkls));
            if copied <= 0 {
                return;
            }
            hkls.truncate(copied as usize);
            self.layouts = hkls.into_iter().map(make_layout_info).collect();
            self.apply_label_overrides();
        }
    }

    pub fn layouts(&self) -> &[LayoutInfo] {
        &self.layouts
    }

    /// Load persisted label overrides for languages that have not been seen yet.
    /// Caching `None` avoids hitting the registry on every language switch.
    pub fn sync_label_overrides<F>(&mut self, mut load: F)
    where
        F: FnMut(u16) -> Option<String>,
    {
        let language_ids: Vec<u16> = self
            .layouts
            .iter()
            .map(|layout| layout.primary_lang_id)
            .collect();
        for language_id in language_ids {
            self.label_overrides.entry(language_id).or_insert_with(|| {
                load(language_id).and_then(|label| validate_custom_label(&label).ok())
            });
        }
        self.apply_label_overrides();
    }

    pub fn set_label_override(&mut self, primary_lang_id: u16, label: Option<String>) {
        self.label_overrides.insert(primary_lang_id, label);
        self.apply_label_overrides();
    }

    fn apply_label_overrides(&mut self) {
        for layout in &mut self.layouts {
            let custom_label = self
                .label_overrides
                .get(&layout.primary_lang_id)
                .and_then(|label| label.as_deref());
            layout.bubble_text =
                resolve_bubble_text(&layout.iso_code, layout.lang_id, custom_label);
        }
    }

    pub fn get_current_layout(&self) -> Option<&LayoutInfo> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return self.layouts.first();
            }
            let thread_id = GetWindowThreadProcessId(hwnd, None);
            let hkl = GetKeyboardLayout(thread_id);
            self.layouts.iter().find(|l| l.hkl == hkl)
        }
    }

    pub fn record_layout_usage(&mut self, hkl: HKL) {
        if let Some(layout) = self.layouts.iter().find(|l| l.hkl == hkl)
            && layout.iso_code != "en"
        {
            self.last_non_english_hkl = Some(hkl);
        }
    }

    pub fn switch_to_next(&self) -> Option<&LayoutInfo> {
        if self.layouts.len() <= 1 {
            return self.layouts.first();
        }
        let current = self.get_current_layout();
        let current_idx = current
            .and_then(|c| self.layouts.iter().position(|l| l.hkl == c.hkl))
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % self.layouts.len();
        let target = &self.layouts[next_idx];
        activate_layout(target);
        Some(target)
    }

    pub fn switch_to_mru(&self) -> Option<&LayoutInfo> {
        if self.layouts.len() <= 1 {
            return self.layouts.first();
        }
        let current = self.get_current_layout();
        let current_is_english = current.is_some_and(|c| c.iso_code == "en");

        if current_is_english {
            // Switch to last non-English
            if let Some(last_hkl) = self.last_non_english_hkl
                && let Some(target) = self.layouts.iter().find(|l| l.hkl == last_hkl)
            {
                activate_layout(target);
                return Some(target);
            }
            // Fallback: first non-English
            if let Some(target) = self.layouts.iter().find(|l| l.iso_code != "en") {
                activate_layout(target);
                return Some(target);
            }
        } else {
            // Switch back to English
            if let Some(target) = self.layouts.iter().find(|l| l.iso_code == "en") {
                activate_layout(target);
                return Some(target);
            }
        }
        self.switch_to_next()
    }

    pub fn get_mru_layouts(&self) -> Vec<LayoutInfo> {
        let mut result = Vec::new();
        if let Some(en) = self.layouts.iter().find(|l| l.iso_code == "en") {
            result.push(en.clone());
        }
        let non_en = self
            .last_non_english_hkl
            .and_then(|hkl| self.layouts.iter().find(|l| l.hkl == hkl))
            .or_else(|| self.layouts.iter().find(|l| l.iso_code != "en"));
        if let Some(l) = non_en {
            result.push(l.clone());
        }
        if result.is_empty() {
            self.layouts.clone()
        } else {
            result
        }
    }
}

fn activate_layout(target: &LayoutInfo) {
    unsafe {
        let hwnd = GetForegroundWindow();
        if !hwnd.is_invalid() {
            let _ = PostMessageW(
                Some(hwnd),
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(target.hkl.0 as isize),
            );
        }
        let _ = PostMessageW(
            Some(HWND_BROADCAST),
            WM_INPUTLANGCHANGEREQUEST,
            WPARAM(0),
            LPARAM(target.hkl.0 as isize),
        );
        let _ = ActivateKeyboardLayout(target.hkl, KLF_SETFORPROCESS);
    }
}

fn make_layout_info(hkl: HKL) -> LayoutInfo {
    let lang_id = (hkl.0 as usize & 0xFFFF) as u16;
    let primary_lang_id = primary_language_id(lang_id);
    let iso_code = get_iso_language_code(lang_id);
    let english_name = get_english_name(lang_id);
    let bubble_text = resolve_bubble_text(&iso_code, lang_id, None);
    LayoutInfo {
        hkl,
        lang_id,
        primary_lang_id,
        iso_code,
        english_name,
        bubble_text,
    }
}

fn get_iso_language_code(lang_id: u16) -> String {
    let mut buf = [0u16; 10];
    let len = get_locale_info(lang_id as u32, LOCALE_SISO639LANGNAME, &mut buf);
    if len > 0 {
        let s = OsString::from_wide(&buf[..(len as usize - 1)]);
        s.to_string_lossy().to_string()
    } else {
        "??".to_string()
    }
}

fn get_english_name(lang_id: u16) -> String {
    let mut buf = [0u16; 256];
    let len = get_locale_info(lang_id as u32, LOCALE_SENGLISHLANGUAGENAME, &mut buf);
    if len > 0 {
        let s = OsString::from_wide(&buf[..(len as usize - 1)]);
        s.to_string_lossy().to_string()
    } else {
        "Unknown".to_string()
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "GetLocaleInfoW"]
    fn GetLocaleInfoW_raw(locale: u32, lctype: u32, lpdata: *mut u16, cchdata: i32) -> i32;
}

// NLS constants not in windows-rs
const LOCALE_SISO639LANGNAME: u32 = 0x0059;
const LOCALE_SENGLISHLANGUAGENAME: u32 = 0x1001;

// Safe wrapper
fn get_locale_info(locale: u32, lctype: u32, buf: &mut [u16]) -> i32 {
    unsafe { GetLocaleInfoW_raw(locale, lctype, buf.as_mut_ptr(), buf.len() as i32) }
}

pub const fn primary_language_id(lang_id: u16) -> u16 {
    lang_id & 0x03FF
}

pub fn resolve_bubble_text(iso_code: &str, lang_id: u16, custom_label: Option<&str>) -> String {
    if let Some(label) = custom_label {
        return label.to_string();
    }

    if (2..=3).contains(&iso_code.len()) && iso_code.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        iso_code.to_ascii_uppercase()
    } else {
        format!("{lang_id:04X}")
    }
}

pub fn validate_custom_label(input: &str) -> Result<String, &'static str> {
    let character_count = input.chars().count();
    if !(1..=3).contains(&character_count) {
        return Err("Enter between 1 and 3 characters.");
    }
    if input
        .chars()
        .any(|character| character.is_whitespace() || !is_printable_label_character(character))
    {
        return Err("Only printable, non-whitespace characters are allowed.");
    }
    Ok(input.to_string())
}

fn is_printable_label_character(character: char) -> bool {
    !character.is_control()
        && !matches!(
            character,
            '\u{00AD}'
                | '\u{061C}'
                | '\u{06DD}'
                | '\u{070F}'
                | '\u{0890}'..='\u{0891}'
                | '\u{08E2}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{206F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
                | '\u{110BD}'
                | '\u{110CD}'
                | '\u{13430}'..='\u{1343F}'
                | '\u{1BCA0}'..='\u{1BCA3}'
                | '\u{1D173}'..='\u{1D17A}'
                | '\u{E0001}'
                | '\u{E0020}'..='\u{E007F}'
        )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn test_layout(lang_id: u16, iso_code: &str) -> LayoutInfo {
        LayoutInfo {
            hkl: HKL::default(),
            lang_id,
            primary_lang_id: primary_language_id(lang_id),
            iso_code: iso_code.to_string(),
            english_name: iso_code.to_string(),
            bubble_text: resolve_bubble_text(iso_code, lang_id, None),
        }
    }

    #[test]
    fn collision_prone_languages_use_distinct_iso_codes() {
        assert_eq!(resolve_bubble_text("en", 0x0409, None), "EN");
        assert_eq!(resolve_bubble_text("ru", 0x0419, None), "RU");
        assert_eq!(resolve_bubble_text("el", 0x0408, None), "EL");
        assert_eq!(resolve_bubble_text("bn", 0x0445, None), "BN");
        assert_eq!(resolve_bubble_text("as", 0x044D, None), "AS");
        assert_eq!(resolve_bubble_text("mk", 0x042F, None), "MK");
        assert_eq!(resolve_bubble_text("mn", 0x0450, None), "MN");
    }

    #[test]
    fn three_letter_and_missing_iso_codes_have_stable_defaults() {
        assert_eq!(resolve_bubble_text("haw", 0x0475, None), "HAW");
        assert_eq!(resolve_bubble_text("??", 0x1234, None), "1234");
    }

    #[test]
    fn primary_language_identity_ignores_regional_sublanguage_bits() {
        assert_eq!(primary_language_id(0x0409), 0x0009);
        assert_eq!(primary_language_id(0x0809), 0x0009);
        assert_ne!(primary_language_id(0x0419), primary_language_id(0x0409));
    }

    #[test]
    fn custom_labels_are_printable_non_whitespace_unicode() {
        assert_eq!(validate_custom_label("Я"), Ok("Я".to_string()));
        assert_eq!(validate_custom_label("語🙂"), Ok("語🙂".to_string()));
        assert_eq!(validate_custom_label("RU"), Ok("RU".to_string()));
        assert!(validate_custom_label("").is_err());
        assert!(validate_custom_label("TOOLONG").is_err());
        assert!(validate_custom_label("R U").is_err());
        assert!(validate_custom_label(" R").is_err());
        assert!(validate_custom_label("R\n").is_err());
        assert!(validate_custom_label("\u{200B}").is_err());
    }

    #[test]
    fn one_override_applies_to_all_regional_layouts_of_a_language() {
        let mut service = LanguageService {
            layouts: vec![test_layout(0x0409, "en"), test_layout(0x0809, "en")],
            last_non_english_hkl: None,
            label_overrides: HashMap::new(),
        };

        service.set_label_override(0x0009, Some("A".to_string()));
        assert!(
            service
                .layouts()
                .iter()
                .all(|layout| layout.bubble_text == "A")
        );

        service.set_label_override(0x0009, None);
        assert!(
            service
                .layouts()
                .iter()
                .all(|layout| layout.bubble_text == "EN")
        );
    }

    #[test]
    fn persisted_overrides_load_once_per_primary_language() {
        let mut service = LanguageService {
            layouts: vec![test_layout(0x0409, "en"), test_layout(0x0809, "en")],
            last_non_english_hkl: None,
            label_overrides: HashMap::new(),
        };
        let reads = Cell::new(0);

        service.sync_label_overrides(|_| {
            reads.set(reads.get() + 1);
            Some("E".to_string())
        });
        service.sync_label_overrides(|_| {
            reads.set(reads.get() + 1);
            Some("ignored".to_string())
        });

        assert_eq!(reads.get(), 1);
        assert!(
            service
                .layouts()
                .iter()
                .all(|layout| layout.bubble_text == "E")
        );
    }

    #[test]
    fn newly_added_layouts_load_their_persisted_override() {
        let mut service = LanguageService {
            layouts: vec![test_layout(0x0409, "en")],
            last_non_english_hkl: None,
            label_overrides: HashMap::new(),
        };
        service.sync_label_overrides(|language_id| match language_id {
            0x0009 => Some("E".to_string()),
            0x0019 => Some("R".to_string()),
            _ => None,
        });

        service.layouts.push(test_layout(0x0419, "ru"));
        service.sync_label_overrides(|language_id| match language_id {
            0x0019 => Some("R".to_string()),
            _ => panic!("previously loaded languages must remain cached"),
        });

        assert_eq!(service.layouts[0].bubble_text, "E");
        assert_eq!(service.layouts[1].bubble_text, "R");
    }
}
