# R1 — Application lifecycle and event dispatch

[Map](README.md) · [Validation](00-validation.md) · [Handoff](AGENT-HANDOFF.md)

Source: [main.rs](../../LanguageBubble/src/main.rs), especially `run`, `AppState`, `with_app`, `msg_wnd_proc`, `process_switch`, and `handle_menu_command`.

Problem: startup/resource ownership, callback dispatch, switching policy, modal dialogs, and preference effects share one module. `with_app` deliberately uses `try_borrow_mut` and deferred state because COM and modal Win32 calls can pump nested messages. That protection must survive extraction.

Ownership: `main.rs` and new `app/` files. Depends on manual baseline; coordinates all root module declarations. Risk: high.

## Checkpoints

1. Document event flow and destruction order in a short module comment. Trace startup failures after each acquired resource; trace deferred update and switch handling, including no-layout/Unused early returns. Record existing coalescing: pending switches are `Option` slots, not a FIFO.
2. Mechanically move application state and handlers behind an `app` module. Keep a small `main` entry calling `app::run` and startup error reporting. Keep the callback ABI, message constants, COM apartment, global mutex name, and message-window identity unchanged. Move tests with their subjects.
3. Separate lifecycle, event dispatch, and menu handling into private child modules only where ownership becomes clearer. Preserve color dialogs outside the mutable app borrow and snapshot creation before displaying the tray menu. Keep teardown of app state before message-window destruction and COM uninitialization.
4. Introduce a small testable deferred-event helper only if it can express current semantics faithfully. Test nested borrowing, last-event coalescing, deferred notification delivery after borrow release, and early-return cases. Do not replace recursion with a queue or add immediate reposting in the extraction commit.

## Acceptance

- Existing theme-message test remains; malformed hook command values remain ignored.
- Common automated gates pass. Manual startup/duplicate instance/exit, rapid switching, modal reentry, and update-during-modal cases match baseline.
- No panic crosses an FFI callback, mutable app borrow spans a color dialog, or immediate deferred-message repost creates a modal loop.
- Startup update scheduling, settings migration ordering, timer routing, and resource cleanup are unchanged.

Reviewer focus: `RefCell` borrow scope, pointer validity, resource drop order, pending-state liveness, and public API impact on R4–R6. If a pending-event bug is reproduced, file a separate fix with a failing regression case rather than silently changing semantics here.

Rollback: revert the extraction checkpoint and its module declarations together; R5 and any later app integration must be reverted or adapted first.
