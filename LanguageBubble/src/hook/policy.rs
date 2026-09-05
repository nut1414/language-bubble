//! Keyboard policy has no OS side effects. The adapter executes the returned
//! fixed-size effect list after releasing its mutable reference to hook state.
use windows::Win32::UI::Input::KeyboardAndMouse::*;

use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyTransition {
    Down,
    Up,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct KeyEvent {
    pub vk: u16,
    pub transition: KeyTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Effect {
    Switch(HookKeyCombo),
    AnyKey,
    ReleaseWin(u16),
    CtrlTap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Suppression {
    Never,
    Always,
    IfReleaseSucceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Decision {
    effects: [Option<Effect>; 2],
    suppression: Suppression,
}

impl Decision {
    const PASS: Self = Self {
        effects: [None, None],
        suppression: Suppression::Never,
    };
    const CONSUME: Self = Self {
        effects: [None, None],
        suppression: Suppression::Always,
    };

    fn switch(combo: HookKeyCombo) -> Self {
        Self {
            effects: [Some(Effect::Switch(combo)), None],
            ..Self::CONSUME
        }
    }

    fn alt_shift() -> Self {
        Self {
            effects: [
                Some(Effect::CtrlTap),
                Some(Effect::Switch(HookKeyCombo::AltShift)),
            ],
            ..Self::PASS
        }
    }

    /// Execute in order. A failed Ctrl tap/post does not cancel Alt+Shift;
    /// failed Win-release injection must forward the real key-up.
    pub(super) fn execute(self, mut perform: impl FnMut(Effect) -> bool) -> bool {
        let mut released = false;
        for effect in self.effects.into_iter().flatten() {
            let succeeded = perform(effect);
            if matches!(effect, Effect::ReleaseWin(_)) {
                released = succeeded;
            }
        }
        match self.suppression {
            Suppression::Never => false,
            Suppression::Always => true,
            Suppression::IfReleaseSucceeded => released,
        }
    }
}

pub(super) struct HookPolicy {
    bindings: KeyBindings,
    win_held: bool,
    win_used_for_combo: bool,
    alt_held: bool,
    shift_held: bool,
    space_held: bool,
    caps_held: bool,
    alt_shift_primed: bool,
    alt_shift_consumed: bool,
}

impl HookPolicy {
    pub(super) fn new(bindings: KeyBindings) -> Self {
        Self {
            bindings,
            win_held: false,
            win_used_for_combo: false,
            alt_held: false,
            shift_held: false,
            space_held: false,
            caps_held: false,
            alt_shift_primed: false,
            alt_shift_consumed: false,
        }
    }

    pub(super) fn set_mode(&mut self, combo: HookKeyCombo, mode: SwitchMode) {
        self.bindings.set_switch_mode(combo, mode);
    }

    fn release_captured_space(&mut self, vk: u16, is_up: bool) -> bool {
        if vk == VK_SPACE.0 && is_up && self.space_held {
            self.space_held = false;
            true
        } else {
            false
        }
    }

    pub(super) fn handle(&mut self, event: KeyEvent) -> Decision {
        let vk = event.vk;
        let is_down = event.transition == KeyTransition::Down;
        let is_up = event.transition == KeyTransition::Up;

        // Release captured Space even if Win was released or disabled first.
        if self.release_captured_space(vk, is_up) {
            return Decision::CONSUME;
        }

        if vk == VK_LWIN.0 || vk == VK_RWIN.0 {
            if is_down {
                self.win_held = true;
                self.win_used_for_combo = false;
            } else if is_up {
                self.win_held = false;
                if self.win_used_for_combo {
                    self.win_used_for_combo = false;
                    return Decision {
                        effects: [Some(Effect::ReleaseWin(vk)), None],
                        suppression: Suppression::IfReleaseSucceeded,
                    };
                }
            }
            return Decision::PASS;
        }

        if vk == VK_SPACE.0
            && self.win_held
            && self.bindings.get(HookKeyCombo::WinSpace).switch_mode != SwitchMode::Unused
        {
            if is_down && !self.space_held {
                self.space_held = true;
                self.win_used_for_combo = true;
                if self.alt_held && self.shift_held {
                    self.alt_shift_consumed = true;
                }
                return Decision::switch(HookKeyCombo::WinSpace);
            } else if is_up {
                self.space_held = false;
            }
            return Decision::CONSUME;
        }

        if vk == VK_LMENU.0 || vk == VK_RMENU.0 || vk == VK_MENU.0 {
            let mut decision = Decision::PASS;
            if is_down {
                let was_held = self.alt_held;
                self.alt_held = true;
                if !was_held && self.shift_held {
                    self.alt_shift_primed = true;
                    self.alt_shift_consumed = false;
                }
            } else if is_up {
                self.alt_held = false;
                if self.alt_shift_primed
                    && !self.alt_shift_consumed
                    && self.bindings.get(HookKeyCombo::AltShift).switch_mode != SwitchMode::Unused
                {
                    decision = Decision::alt_shift();
                }
                self.alt_shift_primed = false;
                self.alt_shift_consumed = false;
            }
            return decision;
        }

        if vk == VK_LSHIFT.0 || vk == VK_RSHIFT.0 || vk == VK_SHIFT.0 {
            let mut decision = Decision::PASS;
            if is_down {
                let was_held = self.shift_held;
                self.shift_held = true;
                if !was_held && self.alt_held {
                    self.alt_shift_primed = true;
                    self.alt_shift_consumed = false;
                }
            } else if is_up {
                self.shift_held = false;
                if self.alt_shift_primed
                    && !self.alt_shift_consumed
                    && self.bindings.get(HookKeyCombo::AltShift).switch_mode != SwitchMode::Unused
                {
                    decision = Decision::alt_shift();
                }
                self.alt_shift_primed = false;
                self.alt_shift_consumed = false;
            }
            return decision;
        }

        if vk == VK_CAPITAL.0 {
            if self.bindings.get(HookKeyCombo::CapsLock).switch_mode == SwitchMode::Unused {
                return Decision::PASS;
            }
            if is_down && !self.caps_held {
                self.caps_held = true;
                if self.alt_held && self.shift_held {
                    self.alt_shift_consumed = true;
                }
                return Decision::switch(HookKeyCombo::CapsLock);
            } else if is_up {
                self.caps_held = false;
            }
            return Decision::CONSUME;
        }

        if is_down {
            if self.alt_held && self.shift_held {
                self.alt_shift_consumed = true;
            }
            return Decision {
                effects: [Some(Effect::AnyKey), None],
                ..Decision::PASS
            };
        }
        Decision::PASS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> HookPolicy {
        let mut policy = HookPolicy::new(KeyBindings::default());
        policy.set_mode(HookKeyCombo::WinSpace, SwitchMode::AllLanguage);
        policy.set_mode(HookKeyCombo::AltShift, SwitchMode::Mru);
        policy
    }

    fn event(policy: &mut HookPolicy, key: VIRTUAL_KEY, down: bool) -> Decision {
        policy.handle(KeyEvent {
            vk: key.0,
            transition: if down {
                KeyTransition::Down
            } else {
                KeyTransition::Up
            },
        })
    }

    #[test]
    fn caps_lock_suppresses_repeats_but_switches_once_per_press() {
        let mut policy = enabled();
        for _ in 0..2 {
            assert_eq!(
                event(&mut policy, VK_CAPITAL, true),
                Decision::switch(HookKeyCombo::CapsLock)
            );
            assert_eq!(event(&mut policy, VK_CAPITAL, true), Decision::CONSUME);
            assert_eq!(event(&mut policy, VK_CAPITAL, false), Decision::CONSUME);
        }
    }

    #[test]
    fn win_space_pairs_releases_in_both_orders_and_handles_injection_failure() {
        for win_key in [VK_LWIN, VK_RWIN] {
            for win_first in [false, true] {
                let mut policy = enabled();
                assert_eq!(event(&mut policy, win_key, true), Decision::PASS);
                assert_eq!(
                    event(&mut policy, VK_SPACE, true),
                    Decision::switch(HookKeyCombo::WinSpace)
                );
                assert_eq!(event(&mut policy, VK_SPACE, true), Decision::CONSUME);
                if !win_first {
                    assert_eq!(event(&mut policy, VK_SPACE, false), Decision::CONSUME);
                }
                let release = event(&mut policy, win_key, false);
                for succeeded in [false, true] {
                    let mut effects = Vec::new();
                    assert_eq!(
                        release.execute(|effect| {
                            effects.push(effect);
                            succeeded
                        }),
                        succeeded
                    );
                    assert_eq!(effects, [Effect::ReleaseWin(win_key.0)]);
                }
                if win_first {
                    assert_eq!(event(&mut policy, VK_SPACE, false), Decision::CONSUME);
                }
                assert_eq!(event(&mut policy, win_key, false), Decision::PASS);
                assert_eq!(event(&mut policy, VK_SPACE, false), Decision::PASS);
            }
        }
    }

    #[test]
    fn alt_shift_switches_on_first_release_in_either_order() {
        for (alt, shift) in [
            (VK_MENU, VK_SHIFT),
            (VK_LMENU, VK_LSHIFT),
            (VK_RMENU, VK_RSHIFT),
        ] {
            for alt_first in [false, true] {
                for release_alt_first in [false, true] {
                    let mut policy = enabled();
                    let press = if alt_first {
                        [alt, shift]
                    } else {
                        [shift, alt]
                    };
                    for key in press {
                        assert_eq!(event(&mut policy, key, true), Decision::PASS);
                        assert_eq!(event(&mut policy, key, true), Decision::PASS);
                    }
                    let release = if release_alt_first {
                        [alt, shift]
                    } else {
                        [shift, alt]
                    };
                    let decision = event(&mut policy, release[0], false);
                    let mut effects = Vec::new();
                    // A failed Ctrl tap still posts the switch, and modifiers pass through.
                    assert!(!decision.execute(|effect| {
                        effects.push(effect);
                        false
                    }));
                    assert_eq!(
                        effects,
                        [Effect::CtrlTap, Effect::Switch(HookKeyCombo::AltShift)]
                    );
                    assert_eq!(event(&mut policy, release[1], false), Decision::PASS);
                }
            }
        }
    }

    #[test]
    fn typing_or_another_switch_consumes_primed_alt_shift() {
        for key in [VK_A, VK_CAPITAL, VK_SPACE] {
            let mut policy = enabled();
            if key == VK_SPACE {
                event(&mut policy, VK_LWIN, true);
            }
            event(&mut policy, VK_MENU, true);
            event(&mut policy, VK_SHIFT, true);
            let decision = event(&mut policy, key, true);
            let expected = if key == VK_A {
                Decision {
                    effects: [Some(Effect::AnyKey), None],
                    ..Decision::PASS
                }
            } else {
                Decision::switch(if key == VK_SPACE {
                    HookKeyCombo::WinSpace
                } else {
                    HookKeyCombo::CapsLock
                })
            };
            assert_eq!(decision, expected);
            assert_eq!(event(&mut policy, VK_MENU, false), Decision::PASS);
            assert_eq!(event(&mut policy, VK_SHIFT, false), Decision::PASS);
        }
    }

    #[test]
    fn disabled_bindings_preserve_native_keys_and_typing_notification() {
        let mut policy = enabled();
        for combo in HookKeyCombo::ALL {
            policy.set_mode(combo, SwitchMode::Unused);
        }
        for key in [VK_CAPITAL, VK_LWIN, VK_MENU, VK_SHIFT] {
            assert_eq!(event(&mut policy, key, true), Decision::PASS);
        }
        assert_eq!(
            event(&mut policy, VK_SPACE, true),
            Decision {
                effects: [Some(Effect::AnyKey), None],
                ..Decision::PASS
            }
        );
        for key in [VK_CAPITAL, VK_LWIN, VK_SPACE, VK_MENU, VK_SHIFT] {
            assert_eq!(event(&mut policy, key, false), Decision::PASS);
        }
    }

    #[test]
    fn disabling_win_space_while_held_still_consumes_captured_space_up() {
        let mut policy = enabled();
        event(&mut policy, VK_LWIN, true);
        event(&mut policy, VK_SPACE, true);
        policy.set_mode(HookKeyCombo::WinSpace, SwitchMode::Unused);
        assert_eq!(event(&mut policy, VK_SPACE, false), Decision::CONSUME);
        assert!(event(&mut policy, VK_LWIN, false).execute(|_| true));
    }

    #[test]
    fn caps_binding_change_preserves_existing_held_flag_behavior() {
        let mut policy = enabled();
        event(&mut policy, VK_CAPITAL, true);
        policy.set_mode(HookKeyCombo::CapsLock, SwitchMode::Unused);
        assert_eq!(event(&mut policy, VK_CAPITAL, false), Decision::PASS);
        policy.set_mode(HookKeyCombo::CapsLock, SwitchMode::Mru);
        // Existing behavior: the disabled key-up did not clear caps_held.
        assert_eq!(event(&mut policy, VK_CAPITAL, true), Decision::CONSUME);
        event(&mut policy, VK_CAPITAL, false);
        assert_eq!(
            event(&mut policy, VK_CAPITAL, true),
            Decision::switch(HookKeyCombo::CapsLock)
        );
    }

    #[test]
    fn modifier_binding_changes_apply_at_release() {
        for mode in [SwitchMode::Unused, SwitchMode::Mru] {
            let mut policy = enabled();
            event(&mut policy, VK_MENU, true);
            event(&mut policy, VK_SHIFT, true);
            policy.set_mode(HookKeyCombo::AltShift, mode);
            assert_eq!(
                event(&mut policy, VK_MENU, false),
                if mode == SwitchMode::Unused {
                    Decision::PASS
                } else {
                    Decision::alt_shift()
                }
            );
        }
    }

    #[test]
    fn win_repeat_keeps_existing_combo_reset_behavior() {
        let mut policy = enabled();
        event(&mut policy, VK_LWIN, true);
        event(&mut policy, VK_SPACE, true);
        event(&mut policy, VK_LWIN, true);
        assert_eq!(event(&mut policy, VK_LWIN, false), Decision::PASS);
        assert_eq!(event(&mut policy, VK_SPACE, false), Decision::CONSUME);
    }

    #[test]
    fn hook_state_starts_released_with_configured_bindings() {
        let bindings = KeyBindings::default();
        let state = HookPolicy::new(bindings);

        assert_eq!(state.bindings, bindings);
        assert!(!state.win_held);
        assert!(!state.win_used_for_combo);
        assert!(!state.alt_held);
        assert!(!state.shift_held);
        assert!(!state.space_held);
        assert!(!state.caps_held);
        assert!(!state.alt_shift_primed);
        assert!(!state.alt_shift_consumed);
    }

    #[test]
    fn hook_state_updates_only_selected_mode() {
        let mut state = HookPolicy::new(KeyBindings::default());
        state.set_mode(HookKeyCombo::WinSpace, SwitchMode::Mru);

        assert_eq!(
            state.bindings.get(HookKeyCombo::WinSpace).switch_mode,
            SwitchMode::Mru
        );
        assert_eq!(
            state.bindings.get(HookKeyCombo::CapsLock).switch_mode,
            SwitchMode::AllLanguage
        );
    }

    #[test]
    fn captured_space_is_released_even_after_win_is_released_first() {
        let mut state = HookPolicy::new(KeyBindings::default());
        state.space_held = true;
        state.win_held = false;

        assert!(state.release_captured_space(VK_SPACE.0, true));
        assert!(!state.space_held);
        assert!(!state.release_captured_space(VK_SPACE.0, true));
    }
}
