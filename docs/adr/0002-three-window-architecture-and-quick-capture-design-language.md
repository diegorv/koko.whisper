# ADR-0002: Three-window architecture and quick-capture design language

Status: Proposed
Date: 2026-05-22

## Context

The post-refactor UI from ADR-0001 lives in one 400×520 tray popover with two routes (`/` and `/settings`). It is functional but visually unpolished: dark-only theme hardcoded in component styles, PT-BR strings with ASCII transliteration ("Configuracoes", "Transcricoes"), no shared tokens, no list/detail browsing of past sessions (just a flat 20-item filename list that opens Finder on click), settings as a single scrolling pane, and recording controls that share a window with history instead of floating over the user's frontmost app.

The sibling project `quick-capture` (`../quick-capture`) ships a mature design language: light-default with `prefers-color-scheme: dark` override, Inter font stack, violet accent (Tailwind violet-900 / violet-400), shared chrome (segmented nav, status bar, sidebar+detail Settings, two-pane Inbox), and a multi-window architecture (Inbox shell + Composer popover + Settings + Dock) tied together by tray icon + global shortcuts + Accessory→Regular activation policy.

This ADR records the decision to adopt quick-capture's design language and a slimmed-down multi-window architecture in `koko-notes-whisper`. Plan + decision tree are tracked at `.scratch/ui-refactor/PRD.md`.

## Decision

### 1. Three windows, not one

The single tray popover is replaced by three named Tauri windows:

| Window               | Role                                             | Size / chrome                                                                     |
| -------------------- | ------------------------------------------------ | --------------------------------------------------------------------------------- |
| `main`               | History browse + active session shell            | 900×600 default, min 700×450, resizable, standard decorations                     |
| `recording`          | Active recording surface (timer + partial)       | 480×320, frameless, transparent, `always_on_top`, shadow, centered, skip_taskbar  |
| `settings`           | Devices + Folder + Shortcuts + About             | sidebar+detail (~840×600), resizable                                              |

The Dock window from quick-capture is **not** adopted. Whisper sessions run minutes; a 96×96 floating widget is not the right surface for a multi-minute recording task and would duplicate tray functionality.

`close` is intercepted as `hide` on all three windows (mirrors quick-capture ADR-0009). Window bounds persist via `tauri-plugin-window-state`.

Supersedes ADR-0001 §5: the two-route model (`/` + `/settings`) is replaced by three routes (`/`, `/recording`, `/settings`), one per window. `/` becomes the Main shell, not the recording-plus-list combo.

### 2. Activation policy: Accessory by default, Regular when Main visible

The app boots as `Accessory` (no Dock icon, no Cmd+Tab entry, lives in tray). When the user shows `main`, activation flips to `Regular` so the window participates in normal macOS focus. Hiding `main` reverts to `Accessory`. The `recording` and `settings` windows do not flip activation. Same shape as quick-capture ADR-0009.

### 3. quick-capture design language adopted 1:1

A single `globals.css` declares CSS custom properties — colors (`--bg`, `--surface`, `--text`, `--text-muted`, `--accent`, `--border`), spacing scale, radii, font stack (`Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`) — with a `@media (prefers-color-scheme: dark)` block overriding the color set. Component styles consume the tokens. No per-component hex literals.

Colors mirror quick-capture: bg `#f6f6f6` / `#1c1c1c`, surface `#fff` / `#232327`, accent `rgba(76, 29, 149, 1)` / `rgba(167, 139, 250, 1)`. Recording-state red (`#ff4444`) and VU-meter green/orange/red stay as semantic colors outside the token palette.

UI strings switch to English. PT-BR transliteration ("Configuracoes", "Gravando") is replaced with English ("Settings", "Recording") across all components and tray menu items. ADR-0001 PRD identified this as low-priority polish; this refactor brings it into scope because rewriting components touches every label anyway.

### 4. Main window is two-pane history with `Inbox`-style chrome

`main` renders a 40/60 split: list on the left (timestamp + duration + first line of transcript, newest first), detail on the right (Header with `Copy` / `Reveal in Finder` / `Delete` buttons, Meta block with uppercase muted labels, Body that renders multi-track transcripts as `## Microphone` / `## System` chip-headed sections and single-track as plain text).

Status bar at the bottom shows `{N} transcriptions · last {M}m ago` (clone of `InboxList`'s statusbar). Empty state shows a 🎙 glyph + "No transcriptions yet" + the `Cmd+Shift+R` hint. The recovery banner for incomplete sessions (today on `/`) lands at the top of the Main pane — Main is the natural shell for triaging pending work.

Keyboard nav: ↑/↓ moves selection, `Enter` copies the selected transcript to clipboard, `⌘⌫` deletes (with confirm). Search is explicitly deferred (see §8).

### 5. Recording popover takes ownership of the active-session surface

`recording` shows the record button (pulses while recording), the elapsed timer (mm:ss tabular nums), and the live partial transcript with mic/sys track chips. Frameless + transparent + always-on-top so it floats over Zoom / Meet / browser without stealing focus from the call.

On `transcription-complete`, the popover auto-hides and `main` is shown/focused so the user sees the new entry in context. The popover does not display the final transcript inline; the Main detail pane is the source of truth for completed sessions.

### 6. Settings becomes sidebar + detail, grouped

`settings` adopts quick-capture's `SettingsDialog` shape: left sidebar with grouped sections, right pane with cards.

| Group     | Section    | Content                                                                       |
| --------- | ---------- | ----------------------------------------------------------------------------- |
| Capture   | Devices    | Mic + System device selects with VU meters and per-track enable toggles       |
| Capture   | Folder     | Output folder picker                                                          |
| General   | Shortcuts  | Read-only list of global shortcuts                                            |
| General   | Model      | Read-only: `ggml-large-v3-turbo-q5_0`                                         |
| Advanced  | Storage    | Output folder path with `Reveal in Finder` button                             |
| Advanced  | About      | Version + build info (`__BUILD_INFO__`)                                       |

`Updates` is not in scope (ADR-0001 §4); `About` replaces it.

### 7. Global shortcuts and tray menu

| Shortcut         | Scope     | Action                                                            |
| ---------------- | --------- | ----------------------------------------------------------------- |
| `Cmd+Shift+R`    | Global    | Toggle recording. Shows + starts the popover; stops + auto-hides. |
| `Cmd+Shift+H`    | Global    | Toggle `main` visibility.                                         |
| `Cmd+,`          | Main      | Open `settings`.                                                  |

Tray menu:

```
Start / Stop Recording   ⌘⇧R
Show History             ⌘⇧H
Settings…                ⌘,
———
Quit                     ⌘Q
```

The current "open the tray popover" entry is removed because there is no popover anymore.

### 8. Search is deferred; delete is in scope

Two-pane browse without search is acceptable for a personal app at the current scale (hundreds of sessions, not thousands). Substring search across filename + body is a future feature, not refactor work.

`Delete` removes the `.md` file from the output folder (with a confirm dialog). No trash, no undo. Backend grows one command: `delete_transcription(path)`.

### 9. Frontmatter expansion for Meta block; legacy files degrade gracefully

`recording::save_markdown` is amended to write additional frontmatter lines for new sessions: `Duracao`, `Microfone`, `Audio do sistema`, `Trechos`. Legacy files (`Data` + `Idioma` only) are not rewritten; the Detail pane omits Meta rows whose source field is absent. No migration path, no on-disk rewrite of user data (cf. ADR-0001 §3 stance on persistence — this refactor honors it for transcripts even though config remains wipe-and-restart).

### 10. Model lifecycle moves to Rust `setup()`

Today `+page.svelte`'s `onMount` drives `check_model_status` → `download_model` → `initialize_whisper` → `check_incomplete_sessions`. With three windows, the model boot cannot live in any single window's lifecycle (a cold `Cmd+Shift+R` opens `recording` first and must not record without a loaded model).

The model boot moves to Rust `setup()`. The app emits `model-status` (`downloading` / `ready` / `error`) and `model-download-progress` events. Each window subscribes; the recording popover shows a splash + progress overlay if `!ready`; Main shows a splash banner. `check_incomplete_sessions` runs once on app boot; the result is held in `AppState` and surfaced in Main on first show.

### 11. Delivery in tracer-bullet slices

Same convention as ADR-0001: feature branches `refactor/ui-NN-<short-name>` (or `feat/ui-NN-`), Conventional Commits with the slice as scope, one PR per slice, squash-merge.

| Slice | Scope                                                                          |
| ----- | ------------------------------------------------------------------------------ |
| ui-01 | `globals.css` tokens + Inter + dark-via-prefers-color-scheme. Apply lightly.   |
| ui-02 | Window split (3 Tauri windows), tray menu rewrite, EN strings, model boot to Rust setup. |
| ui-03 | Main two-pane shell + Detail pane + frontmatter expansion.                     |
| ui-04 | Delete action + recovery banner in Main.                                       |
| ui-05 | Polish: status bar, empty states, error states, kb nav, animations.            |

Each slice is independently shippable (app runs on `main` between slices). Slice 1 cannot land alone with EN strings — strings move with the window split in slice 2 so the visible change is concentrated.

## Consequences

- The "tray popover" UX is gone. Tray icon now opens a real `main` window via menu items, not a popover anchored to the tray. Users with `Cmd+Shift+R` muscle memory are unaffected.
- `tauri-plugin-window-state` is added (one dep). Configs are otherwise unchanged.
- ADR-0001 §5 is partially superseded — see §1.
- ADR-0001 §3 (config wipe-and-restart) is honored. Transcripts (the user's actual data, not app state) are not rewritten; only new files carry the expanded frontmatter.
- ADR-0001 §4 (no auto-updater) is honored; `About` replaces `Updates`.
- The `/transcriptions` route deferred by ADR-0001 §5 is now in scope under the `/` path (Main shell). The decision there ("recording + list stay on home") is reversed: list stays on `/`, recording moves to `/recording`.
- Search is explicitly future work. The Main window's chrome leaves room for a search bar (between titlebar and list) but the bar itself is not built.
- Five slices match the size of the change. The 18-slice ADR-0001 plan was for a structural refactor of Rust + Svelte; this is a UI overhaul against a structure that already exists.
