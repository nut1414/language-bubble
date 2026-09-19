#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WinKey {
    Left,
    Right,
}

impl WinKey {
    const fn bit(self) -> u8 {
        match self {
            Self::Left => 0b01,
            Self::Right => 0b10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WinSpaceEvent {
    WinDown(WinKey),
    WinUp(WinKey),
    SpaceDown,
    SpaceUp,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct WinSpaceDecision {
    pub suppress: bool,
    pub switch_layout: bool,
    pub neutralize_start: bool,
}

#[derive(Debug, Default)]
pub(super) struct WinSpaceState {
    held_win_keys: u8,
    space_suppressed: bool,
    win_used_for_combo: bool,
}

#[cfg(feature = "win-space-trace")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct WinSpaceSnapshot {
    pub held_win_keys: u8,
    pub space_suppressed: bool,
    pub win_used_for_combo: bool,
}

impl WinSpaceState {
    #[cfg(feature = "win-space-trace")]
    pub(super) fn snapshot(&self) -> WinSpaceSnapshot {
        WinSpaceSnapshot {
            held_win_keys: self.held_win_keys,
            space_suppressed: self.space_suppressed,
            win_used_for_combo: self.win_used_for_combo,
        }
    }

    pub(super) fn handle(
        &mut self,
        event: WinSpaceEvent,
        interception_enabled: bool,
    ) -> Option<WinSpaceDecision> {
        match event {
            WinSpaceEvent::WinDown(key) => {
                let had_win_held = self.held_win_keys != 0;
                self.held_win_keys |= key.bit();

                // Only initialize a new chord on the first Win-down. Repeated
                // key-down events must not erase a combo that already used Space.
                if !had_win_held {
                    self.win_used_for_combo = self.space_suppressed;
                }

                Some(WinSpaceDecision::default())
            }
            WinSpaceEvent::WinUp(key) => {
                self.held_win_keys &= !key.bit();

                // Keep the chord alive until the final held Windows key is released.
                if self.held_win_keys != 0 {
                    return Some(WinSpaceDecision::default());
                }

                if self.win_used_for_combo {
                    self.win_used_for_combo = false;
                    Some(WinSpaceDecision {
                        suppress: true,
                        neutralize_start: true,
                        ..Default::default()
                    })
                } else {
                    Some(WinSpaceDecision::default())
                }
            }
            WinSpaceEvent::SpaceDown => {
                // Once the initial Space-down has been consumed, own all of
                // its repeats until the matching Space-up. Win may already
                // have been released, or interception may have been disabled
                // while the physical Space key is still held.
                if self.space_suppressed {
                    return Some(WinSpaceDecision {
                        suppress: true,
                        ..Default::default()
                    });
                }

                if !interception_enabled || self.held_win_keys == 0 {
                    return None;
                }

                self.space_suppressed = true;
                self.win_used_for_combo = true;

                Some(WinSpaceDecision {
                    suppress: true,
                    switch_layout: true,
                    ..Default::default()
                })
            }
            WinSpaceEvent::SpaceUp => {
                // Match every suppressed Space-down even if Win was released first
                // or interception was disabled while the chord was still held.
                if self.space_suppressed {
                    self.space_suppressed = false;
                    Some(WinSpaceDecision {
                        suppress: true,
                        ..Default::default()
                    })
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_decision(
        actual: Option<WinSpaceDecision>,
        suppress: bool,
        switch_layout: bool,
        neutralize_start: bool,
    ) {
        assert_eq!(
            actual,
            Some(WinSpaceDecision {
                suppress,
                switch_layout,
                neutralize_start,
            })
        );
    }

    fn press_combo(state: &mut WinSpaceState, key: WinKey) {
        assert_decision(
            state.handle(WinSpaceEvent::WinDown(key), true),
            false,
            false,
            false,
        );
        assert_decision(
            state.handle(WinSpaceEvent::SpaceDown, true),
            true,
            true,
            false,
        );
    }

    #[test]
    fn space_released_before_win_completes_one_combo() {
        let mut state = WinSpaceState::default();
        press_combo(&mut state, WinKey::Left);

        assert_decision(
            state.handle(WinSpaceEvent::SpaceUp, true),
            true,
            false,
            false,
        );
        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Left), true),
            true,
            false,
            true,
        );
    }

    #[test]
    fn win_released_before_space_cleans_up_and_next_combo_works() {
        let mut state = WinSpaceState::default();
        press_combo(&mut state, WinKey::Left);

        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Left), true),
            true,
            false,
            true,
        );
        assert_decision(
            state.handle(WinSpaceEvent::SpaceUp, true),
            true,
            false,
            false,
        );

        press_combo(&mut state, WinKey::Left);
    }

    #[test]
    fn repeated_win_down_does_not_erase_consumed_combo() {
        let mut state = WinSpaceState::default();
        press_combo(&mut state, WinKey::Left);

        assert_decision(
            state.handle(WinSpaceEvent::WinDown(WinKey::Left), true),
            false,
            false,
            false,
        );
        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Left), true),
            true,
            false,
            true,
        );
    }

    #[test]
    fn repeated_space_down_switches_only_once() {
        let mut state = WinSpaceState::default();
        press_combo(&mut state, WinKey::Left);

        assert_decision(
            state.handle(WinSpaceEvent::SpaceDown, true),
            true,
            false,
            false,
        );
    }

    #[test]
    fn repeated_space_down_after_win_release_stays_suppressed() {
        let mut state = WinSpaceState::default();
        press_combo(&mut state, WinKey::Left);

        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Left), true),
            true,
            false,
            true,
        );
        assert_decision(
            state.handle(WinSpaceEvent::SpaceDown, true),
            true,
            false,
            false,
        );
        assert_decision(
            state.handle(WinSpaceEvent::SpaceUp, true),
            true,
            false,
            false,
        );
    }

    #[test]
    fn repeated_space_down_after_disabling_interception_stays_suppressed() {
        let mut state = WinSpaceState::default();
        press_combo(&mut state, WinKey::Left);

        assert_decision(
            state.handle(WinSpaceEvent::SpaceDown, false),
            true,
            false,
            false,
        );
        assert_decision(
            state.handle(WinSpaceEvent::SpaceUp, false),
            true,
            false,
            false,
        );
    }

    #[test]
    fn consecutive_complete_chords_each_switch_once() {
        let mut state = WinSpaceState::default();

        for key in [WinKey::Left, WinKey::Right] {
            press_combo(&mut state, key);
            assert_decision(
                state.handle(WinSpaceEvent::SpaceUp, true),
                true,
                false,
                false,
            );
            assert_decision(
                state.handle(WinSpaceEvent::WinUp(key), true),
                true,
                false,
                true,
            );
        }
    }

    #[test]
    fn left_and_right_win_keys_are_supported() {
        for key in [WinKey::Left, WinKey::Right] {
            let mut state = WinSpaceState::default();
            press_combo(&mut state, key);
            assert_decision(
                state.handle(WinSpaceEvent::SpaceUp, true),
                true,
                false,
                false,
            );
            assert_decision(
                state.handle(WinSpaceEvent::WinUp(key), true),
                true,
                false,
                true,
            );
        }
    }

    #[test]
    fn final_win_release_owns_start_neutralization() {
        let mut state = WinSpaceState::default();
        state.handle(WinSpaceEvent::WinDown(WinKey::Left), true);
        state.handle(WinSpaceEvent::WinDown(WinKey::Right), true);
        state.handle(WinSpaceEvent::SpaceDown, true);

        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Left), true),
            false,
            false,
            false,
        );
        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Right), true),
            true,
            false,
            true,
        );
    }

    #[test]
    fn win_alone_passes_through() {
        let mut state = WinSpaceState::default();

        assert_decision(
            state.handle(WinSpaceEvent::WinDown(WinKey::Left), true),
            false,
            false,
            false,
        );
        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Left), true),
            false,
            false,
            false,
        );
    }

    #[test]
    fn disabled_interception_preserves_native_win_space() {
        let mut state = WinSpaceState::default();

        assert_decision(
            state.handle(WinSpaceEvent::WinDown(WinKey::Left), false),
            false,
            false,
            false,
        );
        assert_eq!(state.handle(WinSpaceEvent::SpaceDown, false), None);
        assert_eq!(state.handle(WinSpaceEvent::SpaceUp, false), None);
        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Left), false),
            false,
            false,
            false,
        );
    }

    #[test]
    fn disabling_mid_chord_still_cleans_up_suppressed_release() {
        let mut state = WinSpaceState::default();
        press_combo(&mut state, WinKey::Left);

        assert_decision(
            state.handle(WinSpaceEvent::WinUp(WinKey::Left), false),
            true,
            false,
            true,
        );
        assert_decision(
            state.handle(WinSpaceEvent::SpaceUp, false),
            true,
            false,
            false,
        );

        state.handle(WinSpaceEvent::WinDown(WinKey::Left), false);
        assert_eq!(state.handle(WinSpaceEvent::SpaceDown, false), None);
    }
}
