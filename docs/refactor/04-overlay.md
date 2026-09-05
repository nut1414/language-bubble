# R4 — Overlay rendering and deterministic animation

[Map](README.md) · [Validation](00-validation.md) · [Handoff](AGENT-HANDOFF.md)

Source: [bubble.rs](../../LanguageBubble/src/bubble.rs), [animation.rs](../../LanguageBubble/src/animation.rs), [bubble_layout.rs](../../LanguageBubble/src/bubble_layout.rs).

Problem: `BubbleWindow` owns native-window/timer behavior, rendering resources, text fitting, theme resolution, placement, and animation state. Placement already has a pure tested boundary; animation reads `Instant` directly and lacks dedicated tests.

Ownership: these three files and private `bubble/` children. Baseline required. Internal extraction can precede R1; coordinate public field/accessor cleanup after R1. Risk: high.

## Checkpoints

1. Add time-controlled animation sampling, such as private `*_at(Instant)` helpers with current production methods delegating to them. Test fade endpoints, interruption continuity, carousel/window slide interruption, completion, and label fallback/transition. Avoid wall-clock sleeps and preserve durations/easing/rounding.
2. Extract Direct2D/DirectWrite resource ownership and text fitting into a renderer child module. Pass an explicit frame snapshot of the state drawing already reads. Preserve factory thread affinity, premultiplied alpha, font sizing, clipping, and target invalidation on drawing errors.
3. Keep native position, monitor/DPI queries, window visibility, and timers in the window controller. Reuse `calculate_show_plan`; do not duplicate its geometry or alter its truncation/clamp order. Keep timers on `msg_hwnd`, not the overlay HWND.
4. After R1, replace externally used mutable fields with the smallest required accessors/setters. Coordinate call-site changes through the integrator; do not make state public just to make extraction compile.

## Acceptance

- All existing geometry and fitted-font tests remain. Animation checks cover exact boundaries and partial/interrupted transitions with tolerances appropriate to floats.
- Drawing recovery retains the current behavior of dropping the render target after a draw error. Resource release remains on the UI thread.
- Common gates and manual display-mode, theme, multi-monitor/DPI, rapid switching, typing dismissal, and focus-preservation cases pass.
- Record visual before/after evidence. A unit-tested placement function alone does not establish unchanged DPI or glyph rendering.

Reviewer focus: DIP versus physical coordinates, COLORREF channel ordering, destruction order, target-dependent resources, and timing introduced by sampling changes. Caret provider behavior is outside this packet. Rollback: revert each renderer/animation checkpoint with corresponding caller updates; leave established layout policy intact.
