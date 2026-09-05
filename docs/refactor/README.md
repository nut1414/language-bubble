# Refactor execution map

Status: R1–R7 implemented in the working tree; manual validation, independent review, and hosted CI execution pending. See the [R1–R3 report](EXECUTION-R1-R3.md) and [R4–R7 report](EXECUTION-R4-R7.md). Inspected 2026-09-05 at commit `4c5f841f0e0c77a3b0107123704f0c578f7dd243` plus the existing working-tree manifest edit.

Objective: make Windows integration boundaries and application behavior easier to test and maintain while preserving keyboard, overlay, persistence, and release behavior. These are source-backed refactor proposals, not a claim that every listed area contains a defect. No zero-defect guarantee is possible; the gates below make regressions detectable and changes reversible.

## Start here

1. Read [baseline and validation](00-validation.md), then capture the missing manual baseline.
2. Assign one packet and the [agent handoff contract](AGENT-HANDOFF.md) to each executor.
3. Have a separate reviewer assess the packet and then the resulting diff. Review can be done by an external agent or a subagent; no agents were dispatched in this planning pass.
4. Integrate only work with recorded evidence. Record unavailable validation as blocked, never passed.

## Work packets

Each numbered step inside a packet is a separate reviewable checkpoint. Land a mechanical extraction before any behavior correction.

| ID | Packet | Priority / risk | Prerequisites | Exclusive source ownership |
| --- | --- | --- | --- | --- |
| R1 | [Application lifecycle and dispatch](01-application.md) | First / high | Manual baseline | `main.rs`, new `app/` |
| R2 | [Keyboard hook state machine](02-keyboard.md) | High / high | Baseline; preserve public hook API | `hook.rs`, new `hook/`; `capslock.rs` only if needed |
| R3 | [Layout selection policy](03-language.md) | High / medium | Baseline | `language.rs`, new `language/` |
| R4 | [Overlay rendering and animation](04-overlay.md) | High / high | Baseline; R1 before public API cleanup | `bubble.rs`, new `bubble/`, `animation.rs`, `bubble_layout.rs` |
| R5 | [Preferences and tray construction](05-settings-tray.md) | Medium / medium | R1 integrated | `settings.rs`, new `settings/`, `tray.rs`, new `tray/` |
| R6 | [Update policy and transport](06-updates.md) | Medium / high | Baseline; R1 for notification integration | `update.rs`, new `update/` |
| R7 | [Release tooling and architecture checks](07-release.md) | Medium / medium | Baseline; final validation after R1–R6 | `scripts/`, `.github/workflows/`, release/build documentation |

All Rust source paths in this table are relative to `LanguageBubble/src/`. The integrator owns root module declarations, `Cargo.toml`, `Cargo.lock`, and shared `types.rs` changes. An executor needing these changes supplies a proposed patch; it must not silently expand ownership.

## Scheduling

- Establish the manual baseline first. R1, R2, R3, R4's internal extraction, R6's internal extraction, and R7's script work can run independently in separate checkouts if their existing public interfaces remain intact.
- In a shared checkout, use one writer at a time. Concurrent review is safe; overlapping edits are not.
- Integrate R1, R2, R3, then R4. Execute R5 after R1. Finish R6 notification integration after R1. Finish R7 with all runtime work integrated.
- Preserve `mod hook;`, `mod language;`, etc. by using child modules under existing facade files. This avoids unnecessary concurrent edits to `main.rs`.
- Rebase each packet on integrated prerequisites and rerun its tests. Each packet gets a separate review and commit; do not combine all seven into one diff.

## Preserve existing investments

`bubble_layout.rs` already contains tested placement/transition policy. `SettingsBackend` already supports registry-free testing; `RegistryKey`, `InstalledHook`, and several other resources already use RAII. `TrayCommand` already has stable ID conversion tests. Build on these boundaries instead of introducing a framework or duplicating them.

Do not migrate away from Rust/Win32, introduce an async runtime, redesign the interface, change hotkey defaults, replace persistence formats, bump versions, or publish a release as part of this refactor.

## Follow-up findings requiring separate decisions

- `caret.rs` temporarily changes thread DPI awareness in some paths, while MSAA/UIA paths set PMv2 without restoring the original context. Audit intended context ownership before adding guards; blanket restoration would change existing behavior.
- `update.rs` uses a hand-written field extractor and detached worker carrying an HWND. Parser strictness, HTTP status handling, and notification after shutdown deserve reproduction and explicit behavior decisions, separate from moving functions.
- `main.rs::process_switch` recursively drains a coalesced pending switch and has early returns. Characterize these paths before attempting an iterative dispatcher.

These are review questions, not verified user-visible failures. Keep `caret.rs`, `registry.rs`, and label tables unchanged unless evidence supports a separately scoped fix.

## Completion tracker

| Packet | Executor | Reviewed base/commit | Validation evidence | Status |
| --- | --- | --- | --- | --- |
| R1 | Codex | Working tree; review pending | [Report](EXECUTION-R1-R3.md) | Implemented; manual acceptance pending |
| R2 | Codex | Working tree; review pending | [Report](EXECUTION-R1-R3.md) | Implemented; manual acceptance pending |
| R3 | Codex | Working tree; review pending | [Report](EXECUTION-R1-R3.md) | Implemented; manual acceptance pending |
| R4 | Codex | Working tree; review pending | [Report](EXECUTION-R4-R7.md) | Implemented; visual acceptance pending |
| R5 | Codex | Working tree; review pending | [Report](EXECUTION-R4-R7.md) | Implemented; desktop acceptance pending |
| R6 | Codex | Working tree; review pending | [Report](EXECUTION-R4-R7.md) | Implemented; integration acceptance pending |
| R7 | Codex | Working tree; review pending | [Report](EXECUTION-R4-R7.md) | Builds/packages validated; hosted CI and launch pending |

Done means all accepted packets are integrated, required checks pass, manual observations are recorded, and the independent reviewer has no unresolved blocking findings. A plan or successful compilation alone is not completion.
