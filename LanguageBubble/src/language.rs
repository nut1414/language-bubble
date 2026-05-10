use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_INPUTLANGCHANGEREQUEST: u32 = 0x0050;
const KLF_SETFORPROCESS: ACTIVATE_KEYBOARD_LAYOUT_FLAGS = ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0x00000100);

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
                self.layouts.clear();
                return;
            }
            let mut hkls = vec![HKL::default(); count];
            GetKeyboardLayoutList(Some(&mut hkls));
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
    let bubble_text = get_bubble_text(&two_letter);
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

fn get_bubble_text(two_letter: &str) -> String {
    match two_letter {
        "en" => "A",
        "ja" => "\u{3042}",    // あ
        "zh" => "\u{4e2d}",    // 中
        "ko" => "\u{ac00}",    // 가
        "th" => "\u{0e01}",    // ก
        "km" => "\u{1780}",    // ក
        "lo" => "\u{0ea5}",    // ລ
        "my" => "\u{1000}",    // က
        "hi" => "\u{0905}",    // अ
        "mr" => "\u{092e}",    // म
        "ne" => "\u{0928}",    // न
        "sa" => "\u{0938}",    // स
        "bn" | "as" => "\u{0985}", // অ
        "gu" => "\u{0a97}",   // ગ
        "pa" => "\u{0a2a}",   // ਪ
        "ta" => "\u{0ba4}",   // த
        "te" => "\u{0c24}",   // త
        "kn" => "\u{0c95}",   // ಕ
        "ml" => "\u{0d2e}",   // മ
        "si" => "\u{0dc3}",   // ස
        "or" => "\u{0b13}",   // ଓ
        "ur" => "\u{0627}",   // ا
        "ar" => "\u{0639}",   // ع
        "fa" => "\u{0641}",   // ف
        "ps" => "\u{067e}",   // پ
        "ug" => "\u{0626}",   // ئ
        "sd" => "\u{0633}",   // س
        "ku" => "\u{06a9}",   // ک
        "he" => "\u{05d0}",   // א
        "yi" => "\u{05d9}",   // י
        "ru" => "\u{0410}",   // А
        "uk" => "\u{0423}",   // У
        "bg" => "\u{0411}",   // Б
        "sr" => "\u{0421}",   // С
        "mk" => "\u{041c}",   // М
        "kk" => "\u{049a}",   // Қ
        "ky" => "\u{041a}",   // К
        "mn" => "\u{041c}",   // М
        "tg" => "\u{0422}",   // Т
        "el" => "\u{0391}",   // Α
        "ka" => "\u{10d0}",   // ა
        "hy" => "\u{0531}",   // Ա
        "bo" => "\u{0f56}",   // བ
        "am" => "\u{12a0}",   // አ
        "ti" => "\u{1275}",   // ት
        "iu" => "\u{1403}",   // ᐃ
        "cr" => "\u{1431}",   // ᐱ
        other => return other.to_uppercase(),
    }
    .to_string()
}
