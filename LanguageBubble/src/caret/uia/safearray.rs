//! Ownership and bounded copying for UI Automation rectangle arrays.
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{
    SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
};

const MAX_BOUNDING_RECT_VALUES: usize = 4096;

struct OwnedSafeArray(*mut SAFEARRAY);

impl OwnedSafeArray {
    /// Takes ownership of a SAFEARRAY returned by a COM method.
    unsafe fn from_raw(value: *mut SAFEARRAY) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }
}

impl Drop for OwnedSafeArray {
    fn drop(&mut self) {
        unsafe {
            let _ = SafeArrayDestroy(self.0);
        }
    }
}

fn safe_array_value_count(lower: i32, upper: i32) -> Option<usize> {
    if upper < lower {
        return None;
    }
    let count = upper.checked_sub(lower)?.checked_add(1)?;
    usize::try_from(count)
        .ok()
        .filter(|count| *count <= MAX_BOUNDING_RECT_VALUES)
}

/// Copies doubles from an owned SAFEARRAY and releases it on every return path.
pub(super) unsafe fn owned_safearray_to_f64s(raw: *mut SAFEARRAY) -> Option<Vec<f64>> {
    unsafe {
        let safe_array = OwnedSafeArray::from_raw(raw)?;
        let lower = SafeArrayGetLBound(safe_array.0, 1).ok()?;
        let upper = SafeArrayGetUBound(safe_array.0, 1).ok()?;
        let count = safe_array_value_count(lower, upper)?;
        let mut result = Vec::with_capacity(count);
        for offset in 0..count {
            let index = lower.checked_add(i32::try_from(offset).ok()?)?;
            let mut val = 0.0f64;
            SafeArrayGetElement(
                safe_array.0,
                &index,
                (&raw mut val).cast::<std::ffi::c_void>(),
            )
            .ok()?;
            result.push(val);
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_array_bounds_are_checked_before_allocation() {
        assert_eq!(safe_array_value_count(0, 3), Some(4));
        assert_eq!(safe_array_value_count(5, 4), None);
        assert_eq!(safe_array_value_count(i32::MIN, i32::MAX), None);
        assert_eq!(
            safe_array_value_count(0, MAX_BOUNDING_RECT_VALUES as i32),
            None
        );
    }
}
