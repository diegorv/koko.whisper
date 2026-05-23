# ui-02: Three-window split + tray menu + EN strings + model boot to Rust setup

Status: ready-for-agent
Reference: ADR-0002 §§1-2, §7, §10, PRD `.scratch/ui-refactor/PRD.md`

## What to build

Replace the single 400×520 tray popover with three named Tauri windows, rewrite the tray menu, switch UI strings to English, and move the model lifecycle out of `+page.svelte`'s `onMount` into Rust `setup()`.

### Windows

| Label       | Route        | Size            | Min size  | Chrome                                                                              |
| ----------- | ------------ | --------------- | --------- | ----------------------------------------------------------------------------------- |
| `main`      | `/`          | 900×600         | 700×450   | Standard decorations, resizable, hidden at start                                    |
| `recording` | `/recording` | 480×320         | —         | Frameless, transparent, always_on_top, shadow, centered, skip_taskbar, hidden       |
| `settings`  | `/settings`  | 840×600         | 640×400   | Standard decorations, resizable, hidden                                             |

- All three intercept `close` as `hide` (mirrors quick-capture's `intercept_close_as_hide` pattern).
- Bounds persist via `tauri-plugin-window-state` (new dep).
- App boots as `Accessory`. Showing `main` flips to `Regular`; hiding `main` flips back to `Accessory`. `recording` and `settings` do not flip activation.

### Tray menu

```
Start / Stop Recording   ⌘⇧R   (label toggles based on current recording state)
Show History             ⌘⇧H
Settings…                ⌘,
———
Quit                     ⌘Q
```

Remove the current "open the tray popover" item — no popover anchored to the tray exists anymore.

### Global shortcuts

- `Cmd+Shift+R` — toggle recording. Shows `recording` window + starts on first press; stops + auto-hides `recording` on second.
- `Cmd+Shift+H` — toggle `main` visibility (show + focus / hide).
- `Cmd+,` — only active when `main` is focused; opens `settings`.

### Routes

- `/` becomes the Main shell. For this slice it can render a placeholder ("History — coming in ui-03") plus the recovery banner if incomplete sessions exist. Recording controls and the transcription list move out.
- `/recording` hosts the recording surface (`RecordingView.svelte`) and nothing else.
- `/settings` keeps `Settings.svelte` as-is structurally.

### Strings

Sweep every visible label across components, tray menu, and error messages from PT-BR to English. Examples (non-exhaustive): "Configuracoes" → "Settings", "Gravando" → "Recording", "Pasta de transcricoes" → "Transcription folder", "Erro ao baixar modelo" → "Failed to download model", "Recuperar transcricao" → "Recover transcription". Keep tone neutral / app-native.

### Model lifecycle

Move `check_model_status` / `download_model` / `initialize_whisper` / `check_incomplete_sessions` out of `+page.svelte`'s `onMount` and into Rust `setup()`. Emit:

- `model-status` with payload `"downloading" | "ready" | "error"` (plus error message on error)
- `model-download-progress` with payload `number` (0..1), unchanged from today

Each window subscribes; while `!ready`, the `recording` window blocks `start_recording` and shows a splash + progress overlay. `main` shows a splash banner area. Incomplete sessions discovered by `check_incomplete_sessions` are stored in `AppState` and surfaced in `main` on first show.

## Acceptance criteria

- [ ] App launches with all three windows registered but hidden (except optional first-run policy)
- [ ] `Cmd+Shift+R` toggles `recording`; recording behaviour matches current behaviour (timer + partial transcripts + final save)
- [ ] `Cmd+Shift+H` toggles `main`; activation flips between `Accessory` and `Regular` accordingly
- [ ] `Cmd+,` opens `settings` when `main` is focused
- [ ] Tray menu shows the four items in the order listed and dispatches correctly
- [ ] Close button on any of the three windows hides instead of destroying — re-opening shows the same window
- [ ] Window sizes / positions persist across app restarts (via `tauri-plugin-window-state`)
- [ ] All visible UI strings are English; tray menu and error messages too
- [ ] Cold start with no cached model: `recording` window shows a download splash + progress; recording is blocked until `model-status` = `ready`
- [ ] Incomplete sessions left by a prior crash are visible in `main` (banner content can be plain text for this slice; ui-04 polishes)
- [ ] `pnpm vitest run` and `pnpm build` pass; Rust tests pass

## Blocked by

ui-01 (design tokens must exist so the new windows inherit the shared palette).
