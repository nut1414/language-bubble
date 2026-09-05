//! Deterministic selection over opaque layout identities. No foreground queries
//! or activation occurs here; MRU means English versus the last non-English.
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;

use super::LayoutInfo;

pub(super) fn next_index(layouts: &[LayoutInfo], current: Option<HKL>) -> Option<usize> {
    match layouts.len() {
        0 => None,
        1 => Some(0),
        count => {
            let index = current
                .and_then(|hkl| layouts.iter().position(|l| l.hkl == hkl))
                .unwrap_or(0);
            Some((index + 1) % count)
        }
    }
}

/// None asks the adapter to use normal cycling, including its fresh foreground
/// query. Do not fold that fallback into this function using a stale current HKL.
pub(super) fn mru_target(
    layouts: &[LayoutInfo],
    current: Option<HKL>,
    remembered: Option<HKL>,
) -> Option<usize> {
    let current_is_english = current
        .and_then(|hkl| layouts.iter().find(|l| l.hkl == hkl))
        .is_some_and(|l| l.two_letter == "en");
    if current_is_english {
        remembered
            .and_then(|hkl| layouts.iter().position(|l| l.hkl == hkl))
            .or_else(|| layouts.iter().position(|l| l.two_letter != "en"))
    } else {
        layouts.iter().position(|l| l.two_letter == "en")
    }
}

pub(super) fn remember_usage(
    layouts: &[LayoutInfo],
    remembered: Option<HKL>,
    used: HKL,
) -> Option<HKL> {
    if layouts
        .iter()
        .find(|l| l.hkl == used)
        .is_some_and(|l| l.two_letter != "en")
    {
        Some(used)
    } else {
        remembered
    }
}

pub(super) fn mru_indices(layouts: &[LayoutInfo], remembered: Option<HKL>) -> Vec<usize> {
    let mut result = Vec::new();
    if let Some(english) = layouts.iter().position(|l| l.two_letter == "en") {
        result.push(english);
    }
    let non_english = remembered
        .and_then(|hkl| layouts.iter().position(|l| l.hkl == hkl))
        .or_else(|| layouts.iter().position(|l| l.two_letter != "en"));
    if let Some(index) = non_english {
        result.push(index);
    }
    if result.is_empty() {
        (0..layouts.len()).collect()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(id: usize, iso: &str) -> LayoutInfo {
        LayoutInfo {
            hkl: HKL(id as *mut _),
            lang_id: (id & 0xffff) as u16,
            two_letter: iso.into(),
            english_name: iso.into(),
            bubble_text: iso.into(),
        }
    }

    #[test]
    fn cycle_handles_empty_single_wraparound_and_unknown_current() {
        assert_eq!(next_index(&[], None), None);
        assert_eq!(next_index(&[layout(1, "en")], None), Some(0));
        let layouts = [layout(1, "en"), layout(2, "th"), layout(3, "ja")];
        for (index, expected) in [(0, 1), (1, 2), (2, 0)] {
            assert_eq!(
                next_index(&layouts, Some(layouts[index].hkl)),
                Some(expected)
            );
        }
        assert_eq!(next_index(&layouts, None), Some(1));
        assert_eq!(next_index(&layouts, Some(HKL(99usize as *mut _))), Some(1));
    }

    #[test]
    fn mru_uses_remembered_non_english_and_first_english() {
        let layouts = [
            layout(1, "en"),
            layout(2, "th"),
            layout(3, "ja"),
            layout(4, "en"),
        ];
        assert_eq!(
            mru_target(&layouts, Some(layouts[0].hkl), Some(layouts[2].hkl)),
            Some(2)
        );
        assert_eq!(
            mru_target(&layouts, Some(layouts[3].hkl), Some(layouts[1].hkl)),
            Some(1)
        );
        assert_eq!(
            mru_target(&layouts, Some(layouts[2].hkl), Some(layouts[2].hkl)),
            Some(0)
        );
        assert_eq!(mru_target(&layouts, None, None), Some(0));
        assert_eq!(
            mru_target(&layouts, Some(HKL(99usize as *mut _)), None),
            Some(0)
        );
    }

    #[test]
    fn removed_history_falls_back_to_first_non_english() {
        let layouts = [layout(1, "en"), layout(2, "th"), layout(3, "ja")];
        for remembered in [None, Some(HKL(99usize as *mut _))] {
            assert_eq!(
                mru_target(&layouts, Some(layouts[0].hkl), remembered),
                Some(1)
            );
            assert_eq!(mru_indices(&layouts, remembered), [0, 1]);
        }
    }

    #[test]
    fn missing_language_group_requests_cycle_fallback() {
        let non_english = [layout(1, "th"), layout(2, "ja")];
        let english = [layout(3, "en"), layout(4, "en")];
        assert_eq!(
            mru_target(&non_english, Some(non_english[0].hkl), None),
            None
        );
        assert_eq!(mru_target(&english, Some(english[0].hkl), None), None);
        assert_eq!(mru_target(&[], None, None), None);
        assert_eq!(mru_indices(&non_english, None), [0]);
        assert_eq!(mru_indices(&english, None), [0]);
        assert!(mru_indices(&[], None).is_empty());
    }

    #[test]
    fn mru_display_is_english_then_remembered_layout() {
        let layouts = [
            layout(1, "th"),
            layout(2, "en"),
            layout(3, "ja"),
            layout(4, "en"),
        ];
        assert_eq!(mru_indices(&layouts, Some(layouts[2].hkl)), [1, 2]);
        assert_eq!(mru_indices(&layouts, None), [1, 0]);
        assert_eq!(mru_indices(&[layout(1, "th")], None), [0]);
    }

    #[test]
    fn usage_preserves_history_for_english_or_unknown_layouts() {
        let layouts = [layout(1, "en"), layout(2, "th"), layout(3, "ja")];
        let remembered = Some(layouts[1].hkl);
        assert_eq!(
            remember_usage(&layouts, remembered, layouts[0].hkl),
            remembered
        );
        assert_eq!(
            remember_usage(&layouts, remembered, HKL(99usize as *mut _)),
            remembered
        );
        assert_eq!(
            remember_usage(&layouts, remembered, layouts[2].hkl),
            Some(layouts[2].hkl)
        );
        assert_eq!(remember_usage(&[], remembered, layouts[2].hkl), remembered);
    }

    #[test]
    fn layout_identity_does_not_collapse_shared_language_ids() {
        let layouts = [
            layout(0x00010409, "en"),
            layout(0x00020409, "en"),
            layout(0x0001041e, "th"),
            layout(0x0002041e, "th"),
        ];
        assert_eq!(next_index(&layouts, Some(layouts[1].hkl)), Some(2));
        let remembered = remember_usage(&layouts, None, layouts[3].hkl);
        assert_eq!(
            mru_target(&layouts, Some(layouts[0].hkl), remembered),
            Some(3)
        );
        assert_eq!(mru_indices(&layouts, remembered), [0, 3]);
    }
}
