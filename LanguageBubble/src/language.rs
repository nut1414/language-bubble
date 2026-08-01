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
    #[allow(dead_code)]
    pub lang_id: u16,
    pub two_letter: String,
    pub english_name: String,
    pub bubble_text: String,
}

pub struct LanguageService {
    layouts: Vec<LayoutInfo>,
    last_non_english_hkl: Option<HKL>,
}

impl LanguageService {
    pub fn new() -> Self {
        let mut svc = Self {
            layouts: Vec::new(),
            last_non_english_hkl: None,
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
        }
    }

    pub fn layouts(&self) -> &[LayoutInfo] {
        &self.layouts
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
            && layout.two_letter != "en"
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
        let current_is_english = current.is_some_and(|c| c.two_letter == "en");

        if current_is_english {
            // Switch to last non-English
            if let Some(last_hkl) = self.last_non_english_hkl
                && let Some(target) = self.layouts.iter().find(|l| l.hkl == last_hkl)
            {
                activate_layout(target);
                return Some(target);
            }
            // Fallback: first non-English
            if let Some(target) = self.layouts.iter().find(|l| l.two_letter != "en") {
                activate_layout(target);
                return Some(target);
            }
        } else {
            // Switch back to English
            if let Some(target) = self.layouts.iter().find(|l| l.two_letter == "en") {
                activate_layout(target);
                return Some(target);
            }
        }
        self.switch_to_next()
    }

    pub fn get_mru_layouts(&self) -> Vec<LayoutInfo> {
        let mut result = Vec::new();
        if let Some(en) = self.layouts.iter().find(|l| l.two_letter == "en") {
            result.push(en.clone());
        }
        let non_en = self
            .last_non_english_hkl
            .and_then(|hkl| self.layouts.iter().find(|l| l.hkl == hkl))
            .or_else(|| self.layouts.iter().find(|l| l.two_letter != "en"));
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
    let two_letter = get_two_letter_iso(lang_id);
    let english_name = get_english_name(lang_id);
    let bubble_text = get_bubble_text(&two_letter, lang_id);
    LayoutInfo {
        hkl,
        lang_id,
        two_letter,
        english_name,
        bubble_text,
    }
}

fn get_two_letter_iso(lang_id: u16) -> String {
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

fn get_bubble_text(iso_code: &str, lang_id: u16) -> String {
    let glyph = match iso_code {
        // These groups previously rendered as identical or near-identical glyphs.
        "en" | "ru" | "el" | "bn" | "as" | "mk" | "mn" => {
            return fallback_label(iso_code, lang_id);
        }
        "ja" => "\u{3042}",
        "zh" => "\u{4E2D}",
        "ko" => "\u{AC00}",
        "th" => "\u{0E01}",
        "km" => "\u{1780}",
        "lo" => "\u{0EA5}",
        "my" => "\u{1000}",
        "hi" => "\u{0905}",
        "mr" => "\u{092E}",
        "ne" => "\u{0928}",
        "sa" => "\u{0938}",
        "gu" => "\u{0A97}",
        "pa" => "\u{0A2A}",
        "ta" => "\u{0BA4}",
        "te" => "\u{0C24}",
        "kn" => "\u{0C95}",
        "ml" => "\u{0D2E}",
        "si" => "\u{0DC3}",
        "or" => "\u{0B13}",
        "ur" => "\u{0627}",
        "ar" => "\u{0639}",
        "fa" => "\u{0641}",
        "ps" => "\u{067E}",
        "ug" => "\u{0626}",
        "sd" => "\u{0633}",
        "ku" => "\u{06A9}",
        "he" => "\u{05D0}",
        "yi" => "\u{05D9}",
        "uk" => "\u{0423}",
        "bg" => "\u{0411}",
        "sr" => "\u{0421}",
        "kk" => "\u{049A}",
        "ky" => "\u{041A}",
        "tg" => "\u{0422}",
        "ka" => "\u{10D0}",
        "hy" => "\u{0531}",
        "bo" => "\u{0F56}",
        "am" => "\u{12A0}",
        "ti" => "\u{1275}",
        "iu" => "\u{1403}",
        "cr" => "\u{1431}",
        _ => return fallback_label(iso_code, lang_id),
    };
    glyph.to_string()
}

fn fallback_label(iso_code: &str, lang_id: u16) -> String {
    if (2..=3).contains(&iso_code.len()) && iso_code.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        iso_code.to_ascii_uppercase()
    } else {
        format!("{lang_id:04X}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinctive_languages_keep_hand_picked_glyphs() {
        assert_eq!(get_bubble_text("ja", 0x0411), "\u{3042}");
        assert_eq!(get_bubble_text("zh", 0x0804), "\u{4E2D}");
        assert_eq!(get_bubble_text("ko", 0x0412), "\u{AC00}");
        assert_eq!(get_bubble_text("th", 0x041E), "\u{0E01}");
        assert_eq!(get_bubble_text("ar", 0x0401), "\u{0639}");
        assert_eq!(get_bubble_text("uk", 0x0422), "\u{0423}");
    }

    #[test]
    fn collision_prone_languages_use_distinct_iso_codes() {
        assert_eq!(get_bubble_text("en", 0x0409), "EN");
        assert_eq!(get_bubble_text("ru", 0x0419), "RU");
        assert_eq!(get_bubble_text("el", 0x0408), "EL");
        assert_eq!(get_bubble_text("bn", 0x0445), "BN");
        assert_eq!(get_bubble_text("as", 0x044D), "AS");
        assert_eq!(get_bubble_text("mk", 0x042F), "MK");
        assert_eq!(get_bubble_text("mn", 0x0450), "MN");
    }

    #[test]
    fn three_letter_and_missing_iso_codes_have_stable_defaults() {
        assert_eq!(get_bubble_text("fr", 0x040C), "FR");
        assert_eq!(get_bubble_text("haw", 0x0475), "HAW");
        assert_eq!(get_bubble_text("??", 0x1234), "1234");
    }
}
