# R4–R7 execution report

[Execution map](README.md) · [R1–R3 report](EXECUTION-R1-R3.md) · [Validation matrix](00-validation.md) · [Reviewer handoff](AGENT-HANDOFF.md)

Date: 2026-09-05. Base commit: `4c5f841f0e0c77a3b0107123704f0c578f7dd243`, plus the uncommitted R1–R3 implementation. R4–R7 are implemented and integrated in the working tree. Manual desktop acceptance, independent review, and execution of the changed hosted CI workflow remain pending.

No commits, tags, uploads, application installation, version bumps in the real repository, or release publication were performed. Cargo dependencies, the lockfile, and the user's manifest edit remain unchanged. Manifest SHA-256: `AC8B135776F6D4A750DE5BD540028656CE37A2AE5335B31422F36B61FF59BFE1`.

## R4 — Overlay and animation

- [animation.rs](../../LanguageBubble/src/animation.rs) retains its public clock-reading API and delegates to private methods accepting `Instant`. Durations, easing, completion boundaries, integer truncation, and interruption rules remain unchanged. Each operation now samples one time consistently rather than making several closely spaced clock reads.
- [bubble/renderer.rs](../../LanguageBubble/src/bubble/renderer.rs) owns Direct2D/DirectWrite factories, target, text format, and text layouts. A read-only `Frame` carries rendering inputs; the animation reference retains the drawing code's existing sampling behavior. Text fitting, drawing calls, colors, and invalidation on drawing errors remain intact.
- [bubble.rs](../../LanguageBubble/src/bubble.rs) still owns window positioning, DPI/monitor queries, visibility, and message-window timers. Its mutable fields are private, with size/display accessors and a display-mode setter used by the application.
- Existing placement and text-fitting tests remain. Four deterministic animation tests cover fade/slide endpoints, interrupted fades and slides, negative-coordinate truncation, and label transition/fallback behavior without sleeping.

Implementation checkpoints 1–4 addressed. Native visual comparisons, mixed-DPI behavior, actual render-target recovery, and focus/typing-dismissal checks remain in the manual matrix.

## R5 — Settings and tray

- [settings/startup.rs](../../LanguageBubble/src/settings/startup.rs) isolates packaged StartupTask and unpackaged Run-key integration behind existing facade reexports. SettingsBackend, defaults, key names, migration behavior, and error reporting remain unchanged.
- [tray/command.rs](../../LanguageBubble/src/tray/command.rs) holds stable command IDs and the existing round-trip/compatibility tests.
- [tray/menu.rs](../../LanguageBubble/src/tray/menu.rs) splits menu construction into language, startup, appearance, binding, preference, advanced, and action sections. Native ordering, labels, choices, checks, and packaged visibility are retained.
- `MenuOwner` owns unattached handles and transfers them to Windows only after successful submenu attachment. This also fixes cleanup on failed construction/attachment: formerly abandoned native menu handles are now destroyed. Successful menu behavior is unchanged.
- Two native-menu tests construct and inspect menus without showing a popup or changing focus. They verify selected values, disabled customization, packaged/pending-update visibility, and destruction of attached submenu handles with their parent. They do not simulate Win32 allocation or attachment failure.

Implementation checkpoints 1–4 addressed. A generic menu-description framework was unnecessary; testing actual hidden menu handles provides coverage of the native builder. Real color-dialog interaction, persistence across restart, and packaged startup registration remain manual checks.

## R6 — Updates

- [update/policy.rs](../../LanguageBubble/src/update/policy.rs) contains the original parser/version functions plus an explicit release decision for persistence and notification.
- [update/transport.rs](../../LanguageBubble/src/update/transport.rs) contains unchanged WinHTTP transport and handle ownership, including host/path, proxy behavior, timeouts, UTF-8 rejection, and 64 KiB response limit.
- [update.rs](../../LanguageBubble/src/update.rs) coordinates injectable fetch, clock, settings, and notification functions. Timestamp persistence stays in the coordinator before the previous-version read, preserving the original effect ordering. The pure release decision therefore only needs tag/current/previous version inputs.
- New tests exercise newer/equal/older/invalid/prerelease tags, unusual leading `v` behavior, failed fetch with no downstream effects, successful write/notification order, failed writes with continued notification, and already-seen releases. Tests make no live HTTP calls or real settings changes.

Implementation checkpoints 1–3 addressed; checkpoint 4 audited with lifetime changes intentionally deferred. The mutex handoff and deferred application delivery remain unchanged. The worker is still detached and may outlive its HWND; cancellation or replacing the notification mechanism would be a separate behavior fix. No join that could block UI shutdown was introduced. HTTP parser/status strictness also remains a separate decision. Actual offline/packaged/modal/shutdown scenarios remain manual validation.

## R7 — Release tooling

- [release-versions.ps1](../../scripts/release-versions.ps1) provides shared version patterns, match validation, and Cargo/lock/MSIX agreement checks. Importing it defines functions only; it performs no filesystem access or mutation.
- [bump-version.ps1](../../scripts/bump-version.ps1) and [package-msix.ps1](../../scripts/package-msix.ps1) use the common definitions. Bump mutation/ShouldProcess remain separate. Packaging cleanup now resolves and checks the generated temporary path before recursive deletion.
- [test-release.ps1](../../scripts/test-release.ps1) runs without extra test dependencies and passed 53 assertions in temporary repositories. Coverage includes clean versus deliberately untracked-dirty state, malformed/missing/duplicate matches, version mismatches, major/minor/patch increments, byte-preserving dry runs, BOM presence, and LF/CRLF preservation. Fixture inputs are ignored through `.git/info/exclude`; the tests create no commits and do not test tracked-file dirtiness separately.
- [ci.yml](../../.github/workflows/ci.yml) now runs the release fixtures and has explicit x64/ARM64 release-build jobs using the same runner/toolchain pattern as the existing manual-build workflow. This is cross-compilation coverage on the hosted runner; native ARM64 execution remains a distinct lane. The workflow has not been dispatched from this session.
- Both current release binaries were built before packaging with `-SkipBuild`. Windows SDK 10.0.26100.0 generated and unpacked the bundle for validation of identity, version, both architectures, executable, and resources. The pre-existing manifest version already agreed with Cargo; no fixture manifest substitution was needed.

Implementation checkpoints 1–5 addressed locally. Packaged launch/installation is not covered by successful package generation and extraction.

## Exact validation results

Rust commands ran from `LanguageBubble/`; PowerShell and git commands ran from the repository root. Offline Rust commands used cached dependencies.

| Command/check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed, exit 0 |
| `cargo clippy --offline --locked --all-targets -- -D warnings` | Passed, exit 0 |
| `cargo test --offline --locked --all-targets --quiet` | 76 passed, exit 0; x64 executable |
| `cargo test --offline --locked --all-targets --target aarch64-pc-windows-msvc --quiet` | 76 passed, exit 0; ARM64 executable actually ran |
| `cargo build --offline --release --locked --target x86_64-pc-windows-msvc` | Passed, exit 0 |
| `cargo build --offline --release --locked --target aarch64-pc-windows-msvc` | Passed, exit 0 |
| `./scripts/test-release.ps1` | 53 assertions passed, exit 0 |
| `./scripts/package-msix.ps1 -SkipBuild -OutputDirectory release/refactor-r4-r7-20260905` | Passed, exit 0; fresh output directory |
| Same package command with existing output | Expected rejection; package hashes unchanged |
| Same package command with `-Force` | Passed; only expected outputs replaced; unrelated sentinel preserved, then removed by the test |
| `git diff --check` | Passed, exit 0 |
| Source-parity assertions against original commit | Passed: HTTP transport, startup routing, tray command mapping, and drawing operations match after normalizing module plumbing/formatting |

All 66 tests from R1–R3 remain, with 10 new Rust tests. Intermediate Clippy findings (redundant field name and test-module ordering) were corrected, then Clippy passed. Environmental Rustup/Git warnings remain; they did not fail these checks. SDK console output contained encoding artifacts, but generation, extraction, identity/version checks, and final exit codes succeeded.

## Generated packages

These are local validation artifacts, not a published release:

- [x64 MSIX](../../release/refactor-r4-r7-20260905/LanguageBubble_0.5.0.0_x64.msix)
- [ARM64 MSIX](../../release/refactor-r4-r7-20260905/LanguageBubble_0.5.0.0_arm64.msix)
- [Combined MSIX bundle](../../release/refactor-r4-r7-20260905/LanguageBubble_0.5.0.0.msixbundle)

Generated packages are ignored by git. These links resolve in this checkout; build the artifacts again when using another checkout.

## Remaining acceptance and handoff

Native desktop control is unavailable in this session. No real global keyboard gestures, modal dialogs, visual comparison, mixed-monitor DPI testing, update networking, startup registration, or packaged application launch was performed. Complete the [manual matrix](00-validation.md#manual-baseline-and-regression-matrix) against the base and refactored builds using an attended desktop. Tests of hidden native menus cover structure and ownership, not full UI interaction.

The changes were self-reviewed. Independent review is still required using the [reviewer prompt](AGENT-HANDOFF.md#reviewer-prompt), including all new untracked source files. Hosted CI execution remains pending. Do not mark the refactor fully accepted solely from this local validation.

## Integration and rollback

All app call-site changes are integrated. Review/commit R4 as animation, bubble/renderer, and app accessor changes together; R5 as settings/startup and tray/command/menu together; R6 as update facade/policy/transport together; R7 as shared release helper, both scripts, fixture tests, CI, and documentation together. No commits were created by this execution.

Rollback a complete packet and its module wiring after checking for subsequent edits. Retain R1–R3 and the user's manifest/README changes. Reverting the release helper requires reverting both imports together. No settings migration rollback is needed. Generated validation artifacts can be removed separately after confirming ownership.
