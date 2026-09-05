mod selection;

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
        self.last_non_english_hkl =
            selection::remember_usage(&self.layouts, self.last_non_english_hkl, hkl);
    }

    pub fn switch_to_next(&self) -> Option<&LayoutInfo> {
        if self.layouts.len() <= 1 {
            return self.layouts.first();
        }
        let current = self.get_current_layout().map(|layout| layout.hkl);
        let next_idx = selection::next_index(&self.layouts, current)?;
        let target = &self.layouts[next_idx];
        activate_layout(target);
        Some(target)
    }

    pub fn switch_to_mru(&self) -> Option<&LayoutInfo> {
        if self.layouts.len() <= 1 {
            return self.layouts.first();
        }
        let current = self.get_current_layout().map(|layout| layout.hkl);
        if let Some(index) =
            selection::mru_target(&self.layouts, current, self.last_non_english_hkl)
        {
            let target = &self.layouts[index];
            activate_layout(target);
            return Some(target);
        }
        // Preserve the fresh foreground query performed by the original fallback.
        self.switch_to_next()
    }

    pub fn get_mru_layouts(&self) -> Vec<LayoutInfo> {
        selection::mru_indices(&self.layouts, self.last_non_english_hkl)
            .into_iter()
            .map(|index| self.layouts[index].clone())
            .collect()
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
    let layout_id = (hkl.0 as usize & 0xFFFF_FFFF) as u32;
    let lang_id = (layout_id & 0xFFFF) as u16;
    let two_letter = get_two_letter_iso(lang_id);
    let english_name = special_layout_info(layout_id)
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| get_english_name(lang_id));
    let bubble_text = get_bubble_text(&two_letter, lang_id, layout_id);
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

fn get_bubble_text(iso_code: &str, lang_id: u16, layout_id: u32) -> String {
    if let Some((glyph, _)) = special_layout_info(layout_id) {
        return glyph.to_string();
    }

    if let Some(label) = script_specific_label(lang_id) {
        return label.to_string();
    }

    let glyph = match iso_code {
        "en" => "A",
        "ja" => "\u{3042}",
        "zh" => "\u{4E2D}",
        "ko" => "\u{D55C}",
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
        "bo" => "\u{0F56}",
        "am" => "\u{12A0}",
        "ti" => "\u{1275}",
        "iu" => "\u{1403}",
        "cr" => "\u{1431}",
        // Use a native-script label only when it stays recognizable and distinct.
        "ru" => "\u{0420}\u{0423}",
        "el" => "\u{03A9}",
        "bn" => "\u{09AC}",
        "as" => "\u{0985}",
        "mk" => "\u{0403}",
        "mn" => "\u{04E8}",
        // Armenian Ayb is a near-lookalike of the English A label.
        "hy" => return fallback_label(iso_code, lang_id, layout_id),
        _ => return fallback_label(iso_code, lang_id, layout_id),
    };
    glyph.to_string()
}

fn script_specific_label(lang_id: u16) -> Option<&'static str> {
    match lang_id {
        // Chinese: keep simultaneously installed Simplified and Traditional sources distinct.
        0x0804 | 0x1004 => Some("\u{7B80}"),
        0x0404 | 0x0C04 | 0x1404 => Some("\u{7E41}"),
        // Languages for which Windows exposes both Latin and a native-script input source.
        0x082C => Some("\u{04D8}"),         // Azerbaijani Cyrillic
        0x141A => Some("BS"),               // Bosnian Latin
        0x201A => Some("\u{0411}\u{0421}"), // Bosnian Cyrillic
        0x081A | 0x181A | 0x241A | 0x2C1A => Some("SR"),
        0x0C1A | 0x1C1A | 0x281A | 0x301A => Some("\u{0421}"),
        0x045D => Some("\u{1403}"), // Inuktitut syllabics
        0x085D => Some("IU"),       // Inuktitut Latin
        0x0450 => Some("\u{04E8}"), // Mongolian Cyrillic
        0x0850 => Some("\u{182E}"), // Traditional Mongolian
        0x0843 => Some("\u{040E}"), // Uzbek Cyrillic
        0x0443 => Some("UZ"),       // Uzbek Latin
        0x105F => Some("\u{2D5C}"), // Tamazight Tifinagh
        0x085F => Some("TZM"),      // Tamazight Latin
        _ => None,
    }
}

fn special_layout_info(layout_id: u32) -> Option<(&'static str, &'static str)> {
    match layout_id {
        // These Windows script keyboards all use LANGID 0C00, so the full layout ID is required.
        0x0014_0C00 => Some(("\u{1E900}", "Adlam")),
        0x000B_0C00 => Some(("\u{1A00}", "Buginese")),
        0x0012_0C00 => Some(("\u{16A0}", "Futhark")),
        0x000C_0C00 => Some(("\u{10330}", "Gothic")),
        0x0011_0C00 => Some(("\u{A98F}", "Javanese")),
        0x0007_0C00 | 0x0008_0C00 => Some(("\u{A4D0}", "Lisu")),
        0x0001_0C00 | 0x0013_0C00 => Some(("\u{1000}", "Myanmar")),
        0x0002_0C00 => Some(("\u{1980}", "New Tai Lue")),
        0x0009_0C00 => Some(("\u{07CA}", "N'Ko")),
        0x0004_0C00 => Some(("\u{1681}", "Ogham")),
        0x000D_0C00 => Some(("\u{1C5A}", "Ol Chiki")),
        0x000F_0C00 => Some(("\u{10300}", "Old Italic")),
        0x0015_0C00 => Some(("\u{104B0}", "Osage")),
        0x000E_0C00 => Some(("\u{10480}", "Osmanya")),
        0x000A_0C00 => Some(("\u{A840}", "Phags-pa")),
        0x0010_0C00 => Some(("\u{110D0}", "Sora Sompeng")),
        0x0003_0C00 => Some(("\u{1950}", "Tai Le")),
        _ => None,
    }
}

fn fallback_label(iso_code: &str, lang_id: u16, layout_id: u32) -> String {
    if (2..=3).contains(&iso_code.len()) && iso_code.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        iso_code.to_ascii_uppercase()
    } else if lang_id == 0x0C00 && layout_id != lang_id as u32 {
        format!("{:04X}", layout_id >> 16)
    } else {
        format!("{lang_id:04X}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinctive_languages_keep_hand_picked_glyphs() {
        assert_eq!(get_bubble_text("en", 0x0409, 0x0000_0409), "A");
        assert_eq!(get_bubble_text("ja", 0x0411, 0x0000_0411), "\u{3042}");
        assert_eq!(get_bubble_text("ko", 0x0412, 0x0000_0412), "\u{D55C}");
        assert_eq!(get_bubble_text("th", 0x041E, 0x0000_041E), "\u{0E01}");
        assert_eq!(get_bubble_text("ar", 0x0401, 0x0000_0401), "\u{0639}");
        assert_eq!(get_bubble_text("he", 0x040D, 0x0000_040D), "\u{05D0}");
        assert_eq!(get_bubble_text("hi", 0x0439, 0x0000_0439), "\u{0905}");
    }

    #[test]
    fn collision_prone_languages_use_distinct_labels() {
        let labels = [
            get_bubble_text("en", 0x0409, 0x0000_0409),
            get_bubble_text("hy", 0x042B, 0x0000_042B),
            get_bubble_text("ru", 0x0419, 0x0000_0419),
            get_bubble_text("el", 0x0408, 0x0000_0408),
            get_bubble_text("bn", 0x0445, 0x0000_0445),
            get_bubble_text("as", 0x044D, 0x0000_044D),
            get_bubble_text("mk", 0x042F, 0x0000_042F),
            get_bubble_text("mn", 0x0450, 0x0000_0450),
        ];

        assert_eq!(
            labels,
            [
                "A",
                "HY",
                "\u{420}\u{423}",
                "\u{3A9}",
                "\u{9AC}",
                "\u{985}",
                "\u{403}",
                "\u{4E8}"
            ]
        );
        for (index, label) in labels.iter().enumerate() {
            assert!(!labels[..index].contains(label));
        }
    }

    #[test]
    fn script_variants_get_script_appropriate_labels() {
        assert_eq!(get_bubble_text("zh", 0x0804, 0x0000_0804), "\u{7B80}");
        assert_eq!(get_bubble_text("zh", 0x0404, 0x0000_0404), "\u{7E41}");
        assert_eq!(get_bubble_text("mn", 0x0450, 0x0000_0450), "\u{4E8}");
        assert_eq!(get_bubble_text("mn", 0x0850, 0x0000_0850), "\u{182E}");
        assert_eq!(get_bubble_text("iu", 0x045D, 0x0000_045D), "\u{1403}");
        assert_eq!(get_bubble_text("iu", 0x085D, 0x0000_085D), "IU");
        assert_eq!(get_bubble_text("sr", 0x0C1A, 0x0000_0C1A), "\u{421}");
        assert_eq!(get_bubble_text("sr", 0x081A, 0x0000_081A), "SR");
        assert_eq!(get_bubble_text("bs", 0x201A, 0x0000_201A), "\u{411}\u{421}");
        assert_eq!(get_bubble_text("bs", 0x141A, 0x0000_141A), "BS");
        assert_eq!(get_bubble_text("az", 0x082C, 0x0000_082C), "\u{4D8}");
        assert_eq!(get_bubble_text("az", 0x042C, 0x0000_042C), "AZ");
        assert_eq!(get_bubble_text("uz", 0x0843, 0x0000_0843), "\u{40E}");
        assert_eq!(get_bubble_text("uz", 0x0443, 0x0000_0443), "UZ");
        assert_eq!(get_bubble_text("tzm", 0x105F, 0x0000_105F), "\u{2D5C}");
        assert_eq!(get_bubble_text("tzm", 0x085F, 0x0000_085F), "TZM");
    }

    #[test]
    fn special_windows_layouts_do_not_collapse_to_0c00() {
        let layouts = [
            (0x0014_0C00, "\u{1E900}"),
            (0x000B_0C00, "\u{1A00}"),
            (0x0012_0C00, "\u{16A0}"),
            (0x000C_0C00, "\u{10330}"),
            (0x0011_0C00, "\u{A98F}"),
            (0x0007_0C00, "\u{A4D0}"),
            (0x0008_0C00, "\u{A4D0}"),
            (0x0001_0C00, "\u{1000}"),
            (0x0013_0C00, "\u{1000}"),
            (0x0002_0C00, "\u{1980}"),
            (0x0009_0C00, "\u{07CA}"),
            (0x0004_0C00, "\u{1681}"),
            (0x000D_0C00, "\u{1C5A}"),
            (0x000F_0C00, "\u{10300}"),
            (0x0015_0C00, "\u{104B0}"),
            (0x000E_0C00, "\u{10480}"),
            (0x000A_0C00, "\u{A840}"),
            (0x0010_0C00, "\u{110D0}"),
            (0x0003_0C00, "\u{1950}"),
        ];

        for (layout_id, expected) in layouts {
            assert_eq!(get_bubble_text("??", 0x0C00, layout_id), expected);
        }
    }

    #[test]
    fn three_letter_and_missing_iso_codes_have_stable_defaults() {
        assert_eq!(get_bubble_text("fr", 0x040C, 0x0000_040C), "FR");
        assert_eq!(get_bubble_text("haw", 0x0475, 0x0000_0475), "HAW");
        assert_eq!(get_bubble_text("??", 0x1234, 0x0000_1234), "1234");
        assert_eq!(get_bubble_text("??", 0x0C00, 0x0016_0C00), "0016");
    }
}
