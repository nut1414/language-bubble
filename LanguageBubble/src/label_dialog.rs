use std::mem;

use windows::Win32::Foundation::*;
use windows::Win32::UI::Controls::EM_SETLIMITTEXT;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::language::validate_custom_label;

const IDD_LANGUAGE_LABEL: usize = 101;
const IDC_LANGUAGE_NAME: i32 = 1001;
const IDC_LABEL_EDIT: i32 = 1002;
const IDC_USE_DEFAULT: i32 = 1003;
const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;
const DWLP_USER_INDEX: WINDOW_LONG_PTR_INDEX =
    WINDOW_LONG_PTR_INDEX((2 * mem::size_of::<isize>()) as i32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelEditResult {
    Save(String),
    Reset,
    Cancel,
}

struct DialogContext {
    language_description: String,
    current_label: String,
    result: LabelEditResult,
}

pub fn show(
    parent: HWND,
    language_name: &str,
    iso_code: &str,
    current_label: &str,
) -> LabelEditResult {
    let mut context = DialogContext {
        language_description: format!("{language_name} ({})", iso_code.to_ascii_uppercase()),
        current_label: current_label.to_string(),
        result: LabelEditResult::Cancel,
    };

    unsafe {
        let Ok(module) = windows::Win32::System::LibraryLoader::GetModuleHandleW(None) else {
            return LabelEditResult::Cancel;
        };
        let template = PCWSTR(std::ptr::without_provenance::<u16>(IDD_LANGUAGE_LABEL));
        let _ = DialogBoxParamW(
            Some(module.into()),
            template,
            Some(parent),
            Some(dialog_proc),
            LPARAM((&mut context as *mut DialogContext) as isize),
        );
    }

    context.result
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    unsafe {
        match message {
            WM_INITDIALOG => {
                SetWindowLongPtrW(hwnd, DWLP_USER_INDEX, lparam.0);
                let Some(context) = dialog_context(hwnd) else {
                    return 0;
                };
                set_dialog_item_text(hwnd, IDC_LANGUAGE_NAME, &context.language_description);
                set_dialog_item_text(hwnd, IDC_LABEL_EDIT, &context.current_label);
                let _ = SendDlgItemMessageW(
                    hwnd,
                    IDC_LABEL_EDIT,
                    EM_SETLIMITTEXT,
                    WPARAM(8),
                    LPARAM(0),
                );
                1
            }
            WM_COMMAND => {
                let command_id = (wparam.0 & 0xFFFF) as i32;
                match command_id {
                    ID_OK => save_label(hwnd),
                    IDC_USE_DEFAULT => close_dialog(hwnd, LabelEditResult::Reset),
                    ID_CANCEL => close_dialog(hwnd, LabelEditResult::Cancel),
                    _ => 0,
                }
            }
            WM_CLOSE => close_dialog(hwnd, LabelEditResult::Cancel),
            _ => 0,
        }
    }
}

unsafe fn save_label(hwnd: HWND) -> isize {
    unsafe {
        let mut buffer = [0u16; 16];
        let length = GetDlgItemTextW(hwnd, IDC_LABEL_EDIT, &mut buffer) as usize;
        let input = String::from_utf16_lossy(&buffer[..length]);
        match validate_custom_label(&input) {
            Ok(label) => close_dialog(hwnd, LabelEditResult::Save(label)),
            Err(message) => {
                let wide = wide_string(message);
                let _ = MessageBoxW(
                    Some(hwnd),
                    PCWSTR(wide.as_ptr()),
                    w!("Language Bubble"),
                    MB_OK | MB_ICONWARNING,
                );
                1
            }
        }
    }
}

unsafe fn close_dialog(hwnd: HWND, result: LabelEditResult) -> isize {
    unsafe {
        if let Some(context) = dialog_context(hwnd) {
            context.result = result;
        }
        let _ = EndDialog(hwnd, 0);
        1
    }
}

unsafe fn dialog_context(hwnd: HWND) -> Option<&'static mut DialogContext> {
    unsafe {
        let pointer = GetWindowLongPtrW(hwnd, DWLP_USER_INDEX) as *mut DialogContext;
        pointer.as_mut()
    }
}

fn set_dialog_item_text(hwnd: HWND, item_id: i32, text: &str) {
    let wide = wide_string(text);
    unsafe {
        let _ = SetDlgItemTextW(hwnd, item_id, PCWSTR(wide.as_ptr()));
    }
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_results_preserve_saved_text_and_reset_intent() {
        assert_eq!(
            LabelEditResult::Save("RU".to_string()),
            LabelEditResult::Save("RU".to_string())
        );
        assert_ne!(LabelEditResult::Reset, LabelEditResult::Cancel);
    }
}
