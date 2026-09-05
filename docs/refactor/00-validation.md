# Baseline and validation contract

[Execution map](README.md) · [Agent handoff](AGENT-HANDOFF.md)

## Observed baseline

- Commit: `4c5f841f0e0c77a3b0107123704f0c578f7dd243`; Cargo package version `0.5.0`, Rust edition 2024, `windows` dependency `0.61`.
- Host reported by user: Windows 11 ARM64. Active Rust toolchain: `stable-x86_64-pc-windows-msvc`. Both x64 and ARM64 targets installed. The test run below exercised the x64 target, not native ARM64.
- Existing user edit: `LanguageBubble.Package/Package.appxmanifest`, two inserted and two deleted lines. SHA-256 at inspection: `AC8B135776F6D4A750DE5BD540028656CE37A2AE5335B31422F36B61FF59BFE1`. Preserve it; do not reset, stage, or synchronize it automatically.

Commands actually run from `LanguageBubble/` on 2026-09-05:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --offline --locked --all-targets -- -D warnings` | Passed |
| `cargo test --offline --locked --all-targets` | 45 passed; 0 failed |

Rustup emitted a home-path canonicalization warning, without failing these commands. Git emitted a global ignore-file permission warning during initial status inspection. Release builds, packaging, native ARM64 execution, and manual UI checks were not performed. Existing CI in [ci.yml](../../.github/workflows/ci.yml) runs formatting, Clippy, and tests on `windows-latest` without an explicit ARM64 target.

## Required automated gates

From the repository root, inspect `git status --short` and `git diff --check`. From `LanguageBubble/`, run:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

Use `--offline` for Clippy/tests when dependencies are cached and network is unavailable; record that choice. Record each exit code independently. Do not interpret the exit code of only the last command as the result of the entire sequence.

For final integration, from `LanguageBubble/`:

```powershell
cargo build --release --locked --target x86_64-pc-windows-msvc
cargo build --release --locked --target aarch64-pc-windows-msvc
cargo test --locked --all-targets --target aarch64-pc-windows-msvc
```

Run ARM64 tests on a compatible Windows ARM64 machine. Cross-compilation alone is not runtime validation. Capture compiler, linker, SDK, and target versions if environment failures occur. Do not fix an environment error by changing product behavior or relaxing warnings.

## Manual baseline and regression matrix

Record app build, Windows version, target architecture, installed layouts, scaling, observed result, and evidence location for each case. Capture before/after using the same setup. Restore changed preferences afterward. Do not synthesize global hotkeys on an unattended user desktop.

| Area | Cases | Invariant / evidence required |
| --- | --- | --- |
| Keyboard | Caps Lock, Win+Space, Alt+Shift; disabled bindings; repeats; both modifier release orders; binding change while held | One intended switch per recognized gesture; no stuck keys; native pass-through when disabled; no unexpected Start/menu activation |
| Layouts | One layout; English plus Thai; at least three layouts; add/remove a layout while running; external switch | Cycle order and English/non-English MRU behavior match baseline; selected label matches selected target |
| Overlay | Simple, carousel, expanded; MRU-only; each size; rapid switching during fades; typing dismissal | Selection, transitions, position, and dismissal match; foreground application retains focus |
| DPI/caret | Notepad and browser edit controls; available modern UIA control; no caret; 100/150/200% scaling; monitors with negative coordinates | Bubble remains on intended monitor and within the work area; compare fallback behavior and text clarity |
| Theme | System light/dark change; explicit light/dark; custom foreground/background; opacity | No color-channel changes, clipping, or stale theme selection |
| Modal/reentry | Switch while tray menu or color dialog is open; update notification during nested message handling | No panic, freeze, lost update state, or change to established event coalescing |
| Lifecycle | Startup, duplicate instance, exit during animation, repeated relaunch | Hook, tray icon, timers, windows, and COM clean up; duplicate-instance behavior preserved |
| Persistence | Restart after changing preferences; fresh/migrated settings via test backend; simulated failed writes | Existing keys/defaults and runtime-versus-persisted behavior preserved |
| Updates | New/same/older/malformed response, offline, disabled checks, packaged app, exit during check | Use fixtures for automation; no unsolicited live requests or downloads; preserve notification and persistence rules |
| Distribution | Fresh local package output for both architectures; packaged and unpackaged launch | Identity/version agreement and startup routing verified; no publishing required |

If equipment for a case is unavailable, name the missing coverage and route it to an equipped reviewer. UI-sensitive packets cannot claim full verification from unit tests alone.

## Rollback

Use one cohesive commit per checkpoint when authorized to commit. Revert only the packet's commits through native git after checking dependent changes; never use `reset --hard` or discard another person's files. Reverse dependent commits first when needed. Before committing, restore unwanted generated output only when its ownership is established. No packet should require registry migration rollback because persistence changes are out of scope.
