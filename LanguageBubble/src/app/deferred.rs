use std::cell::{Cell, RefCell};

use crate::types::HookKeyCombo;

/// Coalesced UI-thread events, deliberately not a FIFO.
pub(super) struct DeferredEvents {
    switch: Cell<Option<HookKeyCombo>>,
    update: Cell<bool>,
}

impl DeferredEvents {
    pub(super) const fn new() -> Self {
        Self {
            switch: Cell::new(None),
            update: Cell::new(false),
        }
    }

    pub(super) fn defer_switch(&self, combo: HookKeyCombo) {
        self.switch.set(Some(combo));
    }

    pub(super) fn take_switch_after(
        &self,
        pending: &mut Option<HookKeyCombo>,
    ) -> Option<HookKeyCombo> {
        pending.take().or_else(|| self.switch.take())
    }

    pub(super) fn defer_update(&self) {
        self.update.set(true);
    }

    pub(super) fn with_state<T, R>(
        &self,
        state: &RefCell<Option<T>>,
        f: impl FnOnce(&mut T) -> R,
        deliver_update: impl FnOnce(),
    ) -> Option<R> {
        // COM/modal APIs can reenter. A nested borrow must not panic across FFI.
        let result = {
            let mut borrow = state.try_borrow_mut().ok()?;
            borrow.as_mut().map(f)
        };
        // Release the borrow before delivery; immediate reposting can livelock
        // a modal message pump. An unavailable state leaves the event pending.
        if result.is_some() && self.update.take() {
            deliver_update();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_borrow_defers_update_until_outer_borrow_is_released() {
        let events = DeferredEvents::new();
        let state = RefCell::new(Some(0));
        let deliveries = Cell::new(0);
        assert_eq!(
            events.with_state(
                &state,
                |value| {
                    *value = 7;
                    assert_eq!(
                        events.with_state(&state, |_| (), || panic!("nested delivery")),
                        None
                    );
                    events.defer_update();
                    events.defer_update();
                    assert_eq!(deliveries.get(), 0);
                },
                || {
                    assert_eq!(*state.borrow(), Some(7));
                    deliveries.set(deliveries.get() + 1);
                },
            ),
            Some(())
        );
        events.with_state(&state, |_| (), || panic!("duplicate delivery"));
        assert_eq!(deliveries.get(), 1);
    }

    #[test]
    fn missing_state_preserves_update_until_state_is_available() {
        let events = DeferredEvents::new();
        let state = RefCell::new(None::<()>);
        events.defer_update();
        assert_eq!(
            events.with_state(&state, |_| (), || panic!("no state")),
            None
        );
        *state.borrow_mut() = Some(());
        let delivered = Cell::new(false);
        events.with_state(&state, |_| (), || delivered.set(true));
        assert!(delivered.get());
    }

    #[test]
    fn switch_coalescing_preserves_local_pending_priority() {
        let events = DeferredEvents::new();
        events.defer_switch(HookKeyCombo::CapsLock);
        events.defer_switch(HookKeyCombo::WinSpace);
        let mut pending = Some(HookKeyCombo::AltShift);
        assert_eq!(
            events.take_switch_after(&mut pending),
            Some(HookKeyCombo::AltShift)
        );
        assert_eq!(
            events.take_switch_after(&mut pending),
            Some(HookKeyCombo::WinSpace)
        );
        assert_eq!(events.take_switch_after(&mut pending), None);
    }

    #[test]
    fn handler_early_return_does_not_drain_switches() {
        let events = DeferredEvents::new();
        let state = RefCell::new(Some(()));
        events.defer_switch(HookKeyCombo::WinSpace);
        // The Unused/no-layout paths return before the explicit switch drain.
        assert_eq!(events.with_state(&state, |_| None::<()>, || {}), Some(None));
        assert_eq!(
            events.take_switch_after(&mut None),
            Some(HookKeyCombo::WinSpace)
        );
    }
}
