# R2 — Keyboard event policy

[Map](README.md) · [Validation](00-validation.md) · [Handoff](AGENT-HANDOFF.md)

Source: [hook.rs](../../LanguageBubble/src/hook.rs), `HookState`, `hook_proc`, `InstalledHook`, and injection helpers; [capslock.rs](../../LanguageBubble/src/capslock.rs).

Problem: modifier tracking, suppression decisions, Win32 posting, and synthetic input occur together inside an unsafe callback. Existing tests cover initial state, mode changes, and captured Space release, leaving gesture sequences largely coupled to Windows.

Ownership: `hook.rs`, new `hook/` children; touch `capslock.rs` only when necessary and documented. Baseline required; public `InstalledHook` API and messages stay stable. Risk: high.

## Checkpoints

1. Add sequence characterization cases around a small extracted policy function: Caps down/repeat/up, Win+Space release in both orders, Alt+Shift in both press/release orders, third-key cancellation, disabled bindings, and changes while held. Preserve current left/right modifier behavior; improving simultaneous left/right handling would be a separate feature/fix.
2. Move modifier/binding state into a pure event reducer. Inputs explicitly represent key, up/down, and applicable event metadata. Output typed effects for switch posting, typing notification, synthetic release/tap, and pass-through versus suppression. Keep effect order explicit.
3. Keep the raw `KBDLLHOOKSTRUCT` decoding, negative-code forwarding, suppression flag, and self-injected tag check in the adapter. Execute effects only after releasing any mutable policy reference that could be reentered by `SendInput`. Preserve failure-dependent forwarding: Win key-up is suppressed only when the release injection succeeds.
4. Keep boxed state lifetime, thread-local pointer registration, and unhook ownership in `InstalledHook`. Review install failure and drop paths; do not replace thread-local state with a process-wide mutable static.

## Acceptance

- Original tests retained; new sequence tests check emitted effects and consumption, including failed injection behavior through a fake executor or explicit completion result.
- No registry, networking, allocation-heavy framework, or blocking operation added to the hook callback.
- Captured Space key-up remains consumed after Win release or a binding change. Tagged synthetic events do not cause recursive switching.
- Common gates and full manual keyboard matrix pass, including native behavior for unused bindings and no unintended Start-menu activation.

Reviewer focus: key-up pairing, autorepeat, injection reentrancy, mutable aliasing, effect ordering, and forwarding on failure. Rollback: revert reducer and callback adapter together; no app call-site changes should be required.
