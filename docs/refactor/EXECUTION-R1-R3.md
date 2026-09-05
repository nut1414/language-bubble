# R1–R3 execution report

[Execution map](README.md) · [Validation matrix](00-validation.md) · [Reviewer handoff](AGENT-HANDOFF.md)

Date: 2026-09-05. Base: `4c5f841f0e0c77a3b0107123704f0c578f7dd243`.

Status: implementation present in the working tree; manual Windows validation and independent review pending. No commits, tags, releases, dependency changes, or version changes were made.

The pre-existing README/planning documents and package-manifest edit were retained. The manifest SHA-256 remains `AC8B135776F6D4A750DE5BD540028656CE37A2AE5335B31422F36B61FF59BFE1`.

## R1 — Application

- [main.rs](../../LanguageBubble/src/main.rs) delegates to the application entry points.
- [app/mod.rs](../../LanguageBubble/src/app/mod.rs) owns application state and documents event flow and destruction ordering.
- [lifecycle.rs](../../LanguageBubble/src/app/lifecycle.rs) contains the original startup, resource guards, message-window creation, and message loop.
- [dispatch.rs](../../LanguageBubble/src/app/dispatch.rs) contains window-message, update, and switch handling; [menu.rs](../../LanguageBubble/src/app/menu.rs) contains tray and color-dialog handlers.
- [deferred.rs](../../LanguageBubble/src/app/deferred.rs) contains the testable borrow/deferred-event boundary. Nested borrows return `None`; update notifications drain only after the mutable borrow is released. Switch events remain coalesced slots, with local pending state taking priority. Unused/no-layout early-return paths remain unchanged.

All four implementation checkpoints are addressed. Tests cover nested borrows, update coalescing/delivery, unavailable state, pending-switch priority, handler return behavior, invalid switch-message values, and the existing theme-message contract. The helper early-return test does not exercise real COM/layout failure paths; those remain manual/source-review coverage.

Startup failure trace: COM failure creates no guard; mutex failure drops COM; message-window failure drops mutex then COM; bubble failure drops the message window before mutex/COM; hook failure drops the tray and bubble before the message window. On normal loop exit/error, application state is cleared first, releasing hook before bubble/tray; local message-window, mutex, and COM guards then drop. The background update worker remains detached as before (R6 concern).

## R2 — Keyboard policy

- [hook.rs](../../LanguageBubble/src/hook.rs) retains installation, callback decoding, synthetic-event filtering, message posting, and input-injection helpers.
- [hook/policy.rs](../../LanguageBubble/src/hook/policy.rs) owns modifier/binding state and produces an ordered, fixed-size effect list. Policy evaluation performs no OS calls or heap allocation.
- The callback releases its mutable state reference before executing effects. Self-tagged events and global suppression still bypass policy. A failed Win-release injection forwards the real key-up; failed Ctrl-tap injection still allows the Alt+Shift notification.

All four implementation checkpoints are addressed. Existing hook tests were retained in the policy module. Added sequences cover Caps repeats, Win+Space release orders, both Win keys, Alt/Shift press/release orders and variants, repeats, third-key cancellation, disabled bindings, mid-gesture binding changes, and injection success/failure ordering.

Characterized existing quirks were deliberately preserved: disabling Caps Lock interception before key-up leaves its held flag set, and repeated Win-down resets the combo-use flag. These tests record current behavior, not an endorsement; any correction belongs in a separately reviewed fix. Synthetic-event bypass is verified by source inspection; real input injection and Start-menu behavior remain manual checks.

## R3 — Layout selection

- [language.rs](../../LanguageBubble/src/language.rs) retains OS enumeration, foreground queries, activation, and label generation.
- [language/selection.rs](../../LanguageBubble/src/language/selection.rs) isolates cycling, preferred MRU target, usage history, and MRU display ordering over opaque HKL identities.
- Empty/single-layout adapters still return without activation. MRU fallback still calls normal cycling, preserving its second foreground query. Enumeration failure still preserves the old list; English/unknown usage does not overwrite remembered non-English history.

All four implementation checkpoints are addressed. Added tests cover empty/single lists, cycle wraparound, unknown current layout, removed history, missing language groups, multiple English layouts, English-first display ordering, and distinct layouts sharing a language ID. Existing glyph/script/fallback-label tests remain intact.

## Validation evidence

Commands run from `LanguageBubble/` unless noted. Offline mode used cached dependencies; tests did not change real registry preferences or synthesize global hotkeys.

| Check | Result |
| --- | --- |
| Original suite after mechanical R1 split | 45 passed, exit 0 |
| R1 deferred-event suite integrated | 49 passed, exit 0 |
| R2/R3 sequence and selection tests integrated | 65 passed, exit 0 |
| `cargo fmt --all -- --check` | Passed, exit 0 |
| `cargo clippy --offline --locked --all-targets -- -D warnings` | Final source passed, exit 0 |
| `cargo test --offline --locked --all-targets` | Final source: 66 passed, exit 0; x64 executable ran on the ARM64 host |
| `cargo test --offline --locked --all-targets --target aarch64-pc-windows-msvc` | 66 passed, exit 0; ARM64 test executable actually ran |
| `cargo build --offline --release --locked --target x86_64-pc-windows-msvc` | Passed, exit 0 |
| `cargo build --offline --release --locked --target aarch64-pc-windows-msvc` | Passed, exit 0 |
| `git diff --check` (repository root) | Passed, exit 0 |
| Source parity script against `git show HEAD:...` | Passed, exit 0: lifecycle/guard ordering, menu handlers, injection helpers, activation/label code match after normalizing whitespace/visibility |

An intermediate Clippy run caught missing entry-point reexports during import cleanup; restored and rerun successfully. Rustup's home-path warning and Git's global-ignore permission warning remain environmental messages, not failed final checks.

All 45 original tests remain, with 21 new tests. Local file links in all 11 refactor documents resolve. Release executables were built for both architectures but were not launched into the user's desktop.

## Remaining acceptance work

The plans requested a manual baseline before refactoring. Native desktop control is unavailable in this session, so execution proceeded with source characterization and automated tests while keeping this gate explicitly open. Compare the base revision and refactored build on an attended Windows desktop using the same settings/layouts.

- R1: duplicate instance, exit during animation, switching while tray/color dialogs are open, deferred update during nested message pumping, and real no-layout/failure handling.
- R2: actual keyboard hook and injected input, disabled native shortcuts, no stuck keys, and no unexpected Start/menu activation.
- R3: observed layout activation, externally changed layouts, dynamic add/remove, and bubble selection/ordering across the installed layouts.
- Independent reviewer: use [the reviewer prompt](AGENT-HANDOFF.md#reviewer-prompt) against this working-tree diff, including untracked new source files. This implementation was self-reviewed; no separate agent review has been claimed.

Packaging, Store submission, and broader R4–R7 work are outside this execution request. Do not mark R1–R3 fully accepted until the relevant manual cases and independent review are recorded.

## Integration and rollback

Public hook/language APIs remain stable; app call sites are already integrated. Shared `types.rs`, Cargo files, and the manifest were not edited by this execution.

Changes are uncommitted. Review R1 (`main.rs` plus all `app/` files), R2 (`hook.rs` plus `hook/policy.rs`), and R3 (`language.rs` plus `language/selection.rs`) as cohesive groups. If later committed, keep one packet per commit and record hashes here. Roll back a whole group, including its module wiring, only after checking for later edits; never discard the existing manifest/README work. R1 rollback must also account for any future R5/R6 app integrations. No persistence rollback is required.
