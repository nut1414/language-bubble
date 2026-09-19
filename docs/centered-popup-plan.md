# Centered / Overlapped Popup — Generic Caret Plan

Branch: `investigate/centered-popup` (investigation only, temp logs not merged).
Root cause: Qt apps (LINE `Qt663QWindowIcon`, OBS `Qt683QWindowIcon`) don't expose a real
caret rect. Win32 reports a `2×2` caret, MSAA returns `0,0,0,0`, UIA `TextPattern` is
present but `GetBoundingRectangles` is empty. Chrome/Edge/Notepad/Obsidian report tall
carets (`h=24-32`) and are unaffected.

Evidence (`%TEMP%\language-bubble-caret.log`):
- LINE search: `rc=(203,111,205,113)` → `w=2 h=2`, `pre==post`, `win_dpi=144` (no double-scale).
  Placed at `y = bottom+6 = top+8`, inside the ~40px input → overlapped.
- LINE empty chat: `gui miss + msaa 0,0,0,0 + uia empty` → fallback
  `rect=(280,922,815,1036) → x=547` (element center).
- OBS: same pattern, `type=50020 rect=(642,337,1525,980) → x=1083 y=980`.
- Thai text case proves expand works: `[1198,972,6,25] → (1166,1003)`.
- Healthy: Chrome `h=30`, Obsidian `h=32`, OpenCode `h=24` via MSAA.

## Plan (generic, no app allowlist — reliability-based)

### 1. Score caret reliability — `LanguageBubble/src/caret.rs`
- Flag tiny: `h < max(8px, 12*dpi_scale)` (or `w<=0 && h<=0`) as `Unreliable`
  (keep X, drop Y). Covers `2×2`.
- Distinguish UIA states: `pattern_missing` vs `pattern_present_but_empty`.
  Only the latter takes the Qt path.
- Priority: `GUI-tall` > `MSAA-tall` > `UIA-rect` > `UIA-expand-char/word/line` >
  `hybrid` > `last-good` > `CenterOnScreen`.

### 2. Fix Y, keep X — `LanguageBubble/src/bubble_layout.rs::place_at_caret`
- New input: `caret_height`, `element_bottom: Option<i32>`.
- If unreliable height: `y_base = element_bottom (UIA BoundingRectangle.bottom)`
  else `caret_top + estimated_line (e.g. 20*dpi)`; `y = y_base + 4*dpi`.
  X unchanged (`caret.x - width/2`).
- Reuse edge-clamp + above/below flip; add overlap guard: if `y` still inside
  `element_rect`, push below `element_bottom`.

### 3. Gate the center fallback — `caret.rs::try_bounding_rect_fallback`
- Use element-center only when `tp2==false && tp==false` (Explorer-style).
  When patterns exist but rects are empty, return `None` → try `DocumentRange` →
  parent chain → last-good → `CenterOnScreen`.
- Extend `point_from_range`: caret → selection →
  `ExpandToEnclosingUnit(Character/Word/Line)` → enclosing element bottom.

### 4. Last-good cache — `LanguageBubble/src/main.rs::process_switch`
- Cache last `Reliable` point per-monitor; use when current is
  `Unreliable/None` instead of jumping to center. Expire on monitor change.

### 5. Verify
- Remove `temp_caret_log`, gate new logs behind `LB_DEBUG=1`.
- Unit: tiny-caret Y, fallback gating, clamp/flip in `bubble_layout.rs` tests.
- Manual: LINE empty chat (was center), LINE search `asdas` (was overlap),
  OBS `Text (GDI+)`; regression Chrome/Edge/Notepad/Obsidian (must stay MSAA path).

Phase order: cleanup → reliability enum → Y-fix → fallback gate → cache → tests.

## Appendix: Y-offset evidence (center issue excluded)

Mechanism: `bubble_layout.rs::place_at_caret` does `y = caret.y + 4*dpi` with no
height sanity check. Qt's `2px` caret sits at the input top, so `top+8` lands
inside the control. Healthy tall carets land below text.

- LINE search: `rc=(203,111,205,113)` → `w=2 h=2` → `pt=(781,439)` →
  `target=(749,445)`. `pre==post`, `win_dpi=144`: no double-scale.
- LINE stickers: `rc=(21,717,23,719)` → `(848,974)` → `(816,980)`.
- LINE chat: `rc=(155,111,157,113)` → `(733,439)` → `(701,445)`.
- X tracks typing (`rc` `203→266` for `asdas`); only height/Y is bogus.
- Healthy same-machine contrast: Chrome `msaa h=30`, Obsidian `h=32`,
  OpenCode `h=24` → `y = top+30+6`, below text.
- Screenshots: LINE search `asdas`, `Enter a message`, Telegram `+66`,
  Calibre search — bubble straddles input text instead of sitting below it.
- Source: `caret.rs::try_gui_thread_info` returns the 2px rect as-is.
- Note: multi-line Qt chat also shows wrong X (line-start vs caret-end);
  out of scope for the Y fix — needs hybrid UIA-X (reverted, see branch log).

## Compatibility guardrails

- Treat only tiny Win32/MSAA caret heights as `Unreliable`; normal caret paths
  keep their existing source priority, X/Y coordinates, and placement math.
- Prefer a tall GUI caret, then a tall MSAA caret, then a direct UIA range.
  A tiny low-level result keeps its X coordinate but cannot suppress a healthy
  result from another provider.
- Preserve the pattern-less UIA element-bounds fallback used by Explorer-style
  controls. Do not use it when a text pattern exists but exposes no rectangle.
- Scope last-good reuse to the same foreground window and monitor, and clear it
  when either changes; this prevents one application from borrowing another's
  caret position.
- Regression coverage must include reliable GUI/MSAA/UIA selection, the
  pattern-less fallback, tiny-caret placement, edge flipping, cache scoping,
  Chrome/Edge/Notepad/Obsidian paths, and the Qt cases above.

## Field feedback (September 19)

- Telegram's phone field now anchors below the field. LINE's multiline composer
  can still overlap; OBS fields sometimes anchor correctly and sometimes fall
  back to the screen center. These are observations from screenshots, not
  confirmed provider traces for the current build.
- The first implementation probed GUI, MSAA, and UIA eagerly on every switch.
  Restore early returns for reliable GUI/MSAA results to protect working apps.
- For an empty UIA text pattern, a compact Edit control can use its bounds as
  an element fallback. A nearby bounded parent may include the composer's
  chrome and provide a safer bottom edge. Large text/document surfaces must
  not use their center as a caret.
- Keep the physical-height cutoff low enough that a normal small-font caret
  remains reliable. The observed Qt caret is 2×2 pixels; use `8*dpi` with an
  8px minimum as the current trial threshold.
- Manual verification is still required for LINE multiline and empty chat,
  both OBS fields, Telegram, and previously healthy Chrome/Edge/Notepad/
  Obsidian. Unit tests cannot establish the UIA rectangle each app exposes.

## Second LINE/OBS trial

- OBS hotkey filtering is now closer to its field, but LINE's multiline
  composer placed the bubble below the bottom edge of a floating chat window.
  The element-bottom rule was valid geometrically yet visually detached from
  the caret.
- For tiny caret and compact Edit fallbacks only, use the foreground window's
  bottom edge as an additional vertical limit. Flip above the field when the
  bubble would leave that window. Reliable caret and pattern-less element
  fallbacks retain their old work-area placement.
- This still needs manual LINE verification. The popup should appear above the
  composer when there is insufficient room below inside the chat window.

## Collapsed-caret follow-up

- User screenshots show LINE's selected-text range anchoring below the text,
  while an ordinary insertion caret still uses the Edit rectangle and flips
  above the composer. The two states expose different UIA geometry.
- An empty `GetBoundingRectangles` SAFEARRAY previously made `point_from_range`
  return before character expansion. Continue probing after an empty array.
- For a collapsed range in a focused compact Edit only, try an expanded
  character or the preceding character's right edge as an estimated insertion
  point. Prefer a direct selected-text rectangle over that estimate, and do
  not cache the estimate as a reliable caret.
- This is a trial until manually checked in LINE's multiline composer; keep
  GUI/MSAA reliable paths and the selected-text rectangle unchanged.

## Telegram regression follow-up

- The first text-neighbor trial overlapped Telegram's single-line phone field.
  It was enabled for every compact Edit, not just multiline composers.
- Restrict the estimated neighbor caret to focused Edit controls whose own or
  nearby bounded composer height is at least `80*dpi`. A short phone-number
  field returns to the existing field-bounds fallback. Direct selected-text
  rectangles and reliable GUI/MSAA results keep their priority.
- Manual confirmation is still needed in both Telegram and LINE. The height
  gate is a scoped heuristic, not proof that every app reports multiline
  status or caret geometry consistently.

## Telegram selection follow-up

- Telegram's ordinary insertion caret now lands below its phone field, and
  LINE's composer works, but selecting multiple characters in Telegram still
  placed the bubble across the highlighted digits.
- For a reliable UIA text rectangle returned from a short focused Edit, keep
  its horizontal text position while using the field bottom as the vertical
  anchor. This avoids covering selected text without changing tall multiline
  composer positions or reliable Win32/MSAA early returns.
- Verify both Telegram caret and selection, plus LINE caret and selection,
  on the resulting ARM64 build.
