# ui-05: Polish — status bar, empty/error states, keyboard nav

Status: ready-for-agent
Reference: ADR-0002 §4, §11, PRD `.scratch/ui-refactor/PRD.md`

## What to build

Final polish pass across Main, Recording, and Settings. Each item below is small in isolation; bundling them in one slice avoids a long tail of tiny PRs.

### Main: status bar

Bottom strip across the full width of `main`. Two stats separated by a `·`:

- `{N} transcriptions` (singular form for 1)
- `last {M}m ago` — relative time since the most recent transcription's date frontmatter. `just now` < 60s, `Xm ago` < 60min, `Xh ago` < 24h, `Xd ago` otherwise. Refresh every 60s while Main is visible.

Hidden when the list is empty.

### Main: empty states

- **No transcriptions ever** (output folder empty): centered glyph (🎙) + headline "No transcriptions yet" + hint "Press ⌘⇧R to start recording." Mirror quick-capture's `empty` block typography (token-driven).
- **No selection** (list has items, none selected): already shipped in ui-03. Verify the copy is "Select a transcription to read." and the typography matches the empty-state pattern.

### Main: error state

- If `get_transcriptions` fails (folder gone, permission denied), the list area shows a centered error block with the error message + a `Retry` button that re-invokes the command. No banner; this replaces the list content.

### Recording: error state

- If `start_recording` errors (model not ready, devices unavailable), the popover shows an inline error strip above the record button with the message + a `Retry` button. Keep the popover visible (don't auto-hide on error).

### Keyboard nav in Main list

- `↑` / `↓` moves selection (wrap or stop at ends — pick stop-at-ends to match macOS Finder).
- `Enter` on a focused row copies the full transcript to the clipboard and briefly flashes the row (~200ms background pulse) as confirmation. Does NOT open the file.
- `⌘⌫` triggers the same confirm flow as the Delete button (already shipped in ui-04 — verify wiring).
- Tab order: search-bar-placeholder (none for now) → list → detail action buttons → detail body (scrollable but not tabbable past Body).

### Animations

- Selection highlight in the list: 80ms ease background transition (matches quick-capture's chips).
- `model-download-progress` bar: existing transition kept.
- No bespoke animations beyond these. Keep motion budget tight.

## Acceptance criteria

- [ ] Status bar renders at the bottom of Main with `{N} transcriptions · last {M}m ago`
- [ ] Status bar hides when there are zero transcriptions
- [ ] Status bar's "last X ago" updates without manual refresh (interval-driven)
- [ ] Empty list state shows the 🎙 + headline + ⌘⇧R hint
- [ ] No-selection detail state shows "Select a transcription to read"
- [ ] `get_transcriptions` failure renders an in-list error block with a working `Retry`
- [ ] `start_recording` failure renders an inline error strip in the recording popover with a working `Retry`; the popover does not auto-hide
- [ ] `↑` / `↓` in the list moves selection; stops at ends
- [ ] `Enter` on a focused row copies the transcript and briefly pulses the row
- [ ] `⌘⌫` still triggers delete confirm (regression check)
- [ ] Tab order matches the spec above
- [ ] Selection background uses the token-driven 80ms transition
- [ ] `pnpm vitest run` and `pnpm build` pass

## Blocked by

ui-04 (status bar shape and delete keyboard binding both depend on the post-ui-04 list selection model).
