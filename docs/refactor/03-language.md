# R3 — Layout selection policy

[Map](README.md) · [Validation](00-validation.md) · [Handoff](AGENT-HANDOFF.md)

Source: [language.rs](../../LanguageBubble/src/language.rs), `LanguageService::{switch_to_next,switch_to_mru,get_mru_layouts,record_layout_usage,refresh_layouts}`.

Problem: next/MRU selection combines deterministic policy with foreground queries and activation. Current tests protect labels, not selection behavior. MRU currently means switching between English and the last non-English layout, not a general recency list.

Ownership: `language.rs` and new private `language/` modules. Baseline required. Preserve `LayoutInfo` and `LanguageService` public interfaces. Risk: medium.

## Checkpoints

1. Extract pure target-index selection over layout metadata, current index/identity, and remembered non-English identity. Keep HKL opaque; do not truncate it to a language ID, since distinct keyboard layouts can share a language.
2. Characterize empty/single lists, cycle wraparound, unknown current layout, missing English, multiple English layouts, absent remembered non-English layout, and MRU display order. Preserve the existing unknown-current next behavior: default index zero then advance when multiple layouts exist.
3. Retain OS enumeration, foreground lookup, and `activate_layout` in the service adapter. Compute target first; activate exactly where the existing methods do. A single-layout return currently does not call activation.
4. Keep label tables and existing label tests intact. Preserve enumeration failure behavior, which leaves the prior list in place. Do not silently clear remembered history or reinterpret a failed activation as a confirmed layout switch.

## Acceptance

- Selection tests use synthetic layouts without changing OS keyboard configuration.
- Existing distinct glyph/script/special-layout tests and all common gates pass.
- Manual cycle/MRU behavior, external layout changes, and dynamic installed-layout changes match baseline. Confirm bubble ordering and selection through the unchanged application caller.

Reviewer focus: identity versus language code, fallback ordering, English/non-English policy, and number/order of activation calls. Rollback: revert policy extraction and adapter calls together; no registry or user-state rollback required.
