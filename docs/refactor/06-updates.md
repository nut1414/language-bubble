# R6 — Update policy, transport, and notification

[Map](README.md) · [Validation](00-validation.md) · [Handoff](AGENT-HANDOFF.md)

Source: [update.rs](../../LanguageBubble/src/update.rs), and application update scheduling/notification in [main.rs](../../LanguageBubble/src/main.rs) before R1.

Problem: HTTP resource handling, response extraction, version comparison, persistence, and worker notification are combined. Existing tests cover parsing/comparison but not the decision to persist and notify.

Ownership: `update.rs` and private `update/` children. Keep its public API while R1 proceeds; coordinate notification integration afterward. Risk: high.

## Checkpoints

1. Move current parsing/comparison functions and tests into a pure policy module without changing accepted inputs. Characterize `is_newer` with empty/invalid current values and leading `v` behavior; do not assume standard semver semantics beyond actual source.
2. Extract a decision function receiving fetched tag, current version, previous last-seen value, and timestamp. Return explicit persistence and notification decisions. Test newer/same/older/invalid inputs and failed fetch. Preserve successful-fetch-only timestamp writes, last-seen comparison, and existing ordering/error continuation.
3. Move WinHTTP fetching and `HttpHandle` ownership into a transport child module. Preserve host/path, timeouts, proxy mode, 64 KiB cap, UTF-8 rejection, and fetch failure behavior. Inject a small fake fetch/notification boundary for workflow tests; no live GitHub requests in tests.
4. Keep the existing mutex handoff during mechanical extraction. After R1, audit delivery when app state is unavailable and when the worker outlives the window. If changing worker lifetime or cancellation, make it a distinct fix with failure/shutdown tests and a defined non-blocking exit contract.

## Acceptance

- Existing parsing/version tests remain; new policy tests assert saved values and emitted notifications, not implementation details.
- Packaged installs still skip background checking; preference and 24-hour scheduling remain in the app unchanged. `pending_from_registry` retains current restoration rules.
- Common gates and offline/disabled/packaged/modal/shutdown validation pass with unavailable cases disclosed.

Separate decisions: a JSON library, HTTP status checks, stricter tag parsing, and shutdown cancellation may be worthwhile, but can change accepted responses or timing. Propose with evidence; do not bundle into a move-only patch.

Reviewer focus: lock scope, notification ordering, worker HWND lifetime, persisted-versus-notified versions, and handle cleanup. Rollback: revert transport/policy extraction together with any coordinated app wiring; separately revert semantic fixes if accepted later.
