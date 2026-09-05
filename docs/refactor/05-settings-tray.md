# R5 — Preference boundaries and tray construction

[Map](README.md) · [Validation](00-validation.md) · [Handoff](AGENT-HANDOFF.md)

Source: [settings.rs](../../LanguageBubble/src/settings.rs), [tray.rs](../../LanguageBubble/src/tray.rs), [types.rs](../../LanguageBubble/src/types.rs) (read-only unless coordinated).

Problem: settings persistence/migration and packaged/unpackaged startup integration share a module; tray command encoding and a large native-menu builder share another. Existing backend and command-ID tests provide useful seams already.

Ownership: `settings.rs`, `tray.rs`, and their new private children. Depends on R1; app changes are integrator-owned. Risk: medium.

## Checkpoints

1. Retain `SettingsStore<B>` and its test backend. Move startup-task/Run-key integration into a private startup child module with facade reexports. Preserve startup task ID, registry paths, serialization, defaults, migration order, and error reporting.
2. Move `TrayCommand` ID mapping and its tests to a private command child module, preserving reexports. Keep all IDs and gaps unchanged.
3. Break `show_context_menu` into named section builders using the existing `TrayMenuSnapshot` (keys, appearance, startup/update, actions). Preserve menu order, labels, checked states, packaged visibility, and command return behavior. Keep parent/submenu handle ownership explicit and destroy each owned menu once.
4. Add targeted snapshot/section tests only where extracting a pure menu description improves coverage. Test packaged versus unpackaged entries, pending-update visibility, binding choices, and selected settings. Avoid building a generic menu framework.

## Acceptance

- Existing migration, read/write-failure, persisted-enum, startup-route, and command ID tests pass unchanged in meaning.
- Test backend is used instead of real registry writes. Failed migration reads still do not write defaults.
- Common gates plus restart persistence, tray checked-state, color dialog cancellation, and packaged/unpackaged startup checks pass.
- Preserve current behavior when runtime preferences change but saving fails; transactional persistence or a new user-facing error flow requires a separate decision.

Reviewer focus: no duplicate settings source of truth, default drift, migration idempotence, native menu ownership, and holding mutable state across modal APIs. Rollback: revert module extraction/section builder changes with app import adjustments; no data-format migration should exist.
