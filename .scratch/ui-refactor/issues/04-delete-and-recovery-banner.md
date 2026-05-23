# ui-04: Delete transcription + recovery banner in Main

Status: ready-for-agent
Reference: ADR-0002 §4, §8, PRD `.scratch/ui-refactor/PRD.md`

## What to build

Enable the `Delete` action on the Detail pane and polish the incomplete-session recovery surface at the top of Main.

### Delete transcription

- Detail pane's `Delete` button is enabled (replacing the placeholder from ui-03).
- Click triggers a confirm dialog: "Delete this transcription? This cannot be undone." with `Cancel` / `Delete` actions. `Delete` is destructive-styled (red accent).
- On confirm, the backend removes the `.md` file from the output folder.
- Optimistic update: the row is removed from the list immediately; the Detail pane clears to the no-selection state.
- After delete, focus shifts to the row that was just below the deleted one (or above if it was the last). If the list is now empty, focus shifts to nowhere.
- Keyboard: `⌘⌫` on the focused list row triggers the same confirm flow.
- New backend command: `delete_transcription(path: String) -> Result<(), String>`. Validates that `path` is inside the configured output folder (no escapes) and that the file ends in `.md`. Returns the path on success so callers can confirm.

### Recovery banner

- The banner that today lives on `/` (incomplete sessions left by a prior crash) is rendered at the top of Main between the title area and the list/detail panes.
- One row per incomplete session. Each row shows: start time + chunk counts (`X audio chunks captured`) and two buttons: `Recover transcription` (kicks `recover_session`) and `Dismiss` (kicks `dismiss_session`).
- Visual style follows quick-capture's emphasized banner pattern (subtle amber background, thin border, rounded). Tokens already in place from ui-01.
- Banner subscribes to a Rust event when recovery completes so the row disappears without a manual refresh.
- If no incomplete sessions, banner does not render at all.

## Acceptance criteria

- [ ] Detail pane's `Delete` button is active and opens a confirm dialog
- [ ] Confirming delete removes the `.md` file from disk, removes the row from the list, and clears the Detail pane to the no-selection state
- [ ] `⌘⌫` on a focused list row triggers the same confirm flow
- [ ] After delete, selection moves to the next sibling row (or previous if last)
- [ ] `delete_transcription` rejects paths outside the configured output folder
- [ ] `delete_transcription` rejects non-`.md` files
- [ ] Recovery banner renders in Main when incomplete sessions exist
- [ ] `Recover transcription` action completes the transcription and the row vanishes; the new transcription appears in the list
- [ ] `Dismiss` action removes the session directory and removes the banner row
- [ ] No banner renders when there are no incomplete sessions
- [ ] `pnpm vitest run` and `pnpm build` pass; Rust tests cover `delete_transcription` happy path + the two rejection cases

## Blocked by

ui-03 (Detail pane and list selection must exist for delete to attach to).
