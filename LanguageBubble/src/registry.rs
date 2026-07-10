use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
use windows::Win32::System::Registry::*;
use windows::core::{PCWSTR, Result};

pub struct RegistryKey {
    handle: HKEY,
}

impl RegistryKey {
    pub fn open(root: HKEY, subkey: PCWSTR, access: REG_SAM_FLAGS) -> Result<Self> {
        unsafe {
            let mut handle = HKEY::default();
            RegOpenKeyExW(root, subkey, Some(0), access, &mut handle).ok()?;
            Ok(Self { handle })
        }
    }

    pub fn open_optional(
        root: HKEY,
        subkey: PCWSTR,
        access: REG_SAM_FLAGS,
    ) -> Result<Option<Self>> {
        match Self::open(root, subkey, access) {
            Ok(key) => Ok(Some(key)),
            Err(error) if is_file_not_found_error(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn create(root: HKEY, subkey: PCWSTR) -> Result<Self> {
        unsafe {
            let mut handle = HKEY::default();
            RegCreateKeyW(root, subkey, &mut handle).ok()?;
            Ok(Self { handle })
        }
    }

    pub fn query_string(&self, name: PCWSTR, capacity: usize) -> Result<Option<String>> {
        unsafe {
            let mut buffer = vec![0u16; capacity];
            let mut size = (buffer.len() * size_of::<u16>()) as u32;
            let mut kind = REG_VALUE_TYPE::default();
            let result = RegQueryValueExW(
                self.handle,
                name,
                None,
                Some(&mut kind),
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            );
            if result.is_ok() && kind == REG_SZ {
                let length = (size as usize / size_of::<u16>()).saturating_sub(1);
                Ok(Some(String::from_utf16_lossy(&buffer[..length])))
            } else if result.is_ok() || is_file_not_found(result) {
                Ok(None)
            } else {
                Err(result.into())
            }
        }
    }

    pub fn query_u32(&self, name: PCWSTR) -> Result<Option<u32>> {
        unsafe {
            let mut value = 0u32;
            let mut size = size_of::<u32>() as u32;
            let mut kind = REG_VALUE_TYPE::default();
            let result = RegQueryValueExW(
                self.handle,
                name,
                None,
                Some(&mut kind),
                Some((&mut value as *mut u32).cast()),
                Some(&mut size),
            );
            if result.is_ok() {
                Ok(Some(value))
            } else if is_file_not_found(result) {
                Ok(None)
            } else {
                Err(result.into())
            }
        }
    }

    pub fn set_string(&self, name: PCWSTR, value: &str) -> Result<()> {
        unsafe {
            let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            RegSetValueExW(
                self.handle,
                name,
                Some(0),
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    wide.as_ptr().cast(),
                    wide.len() * size_of::<u16>(),
                )),
            )
            .ok()
        }
    }

    pub fn value_exists(&self, name: PCWSTR) -> Result<bool> {
        unsafe {
            let mut size = 0u32;
            let result = RegQueryValueExW(self.handle, name, None, None, None, Some(&mut size));
            if result.is_ok() {
                Ok(true)
            } else if is_file_not_found(result) {
                Ok(false)
            } else {
                Err(result.into())
            }
        }
    }

    pub fn delete_value(&self, name: PCWSTR) -> Result<()> {
        unsafe {
            let result = RegDeleteValueW(self.handle, name);
            if result.is_ok() || is_file_not_found(result) {
                Ok(())
            } else {
                Err(result.into())
            }
        }
    }
}

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.handle);
        }
    }
}

fn is_file_not_found(error: WIN32_ERROR) -> bool {
    error == ERROR_FILE_NOT_FOUND
}

fn is_file_not_found_error(error: &windows::core::Error) -> bool {
    WIN32_ERROR::from_error(error).is_some_and(is_file_not_found)
}
