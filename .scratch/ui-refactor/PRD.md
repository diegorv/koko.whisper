# UI refactor — adopt quick-capture design language + three-window architecture

Status: ready-for-agent
Owner: @diegorv
Reference: /grill-me session 2026-05-22, ADR-0002

## Why

Post-ADR-0001 the structural refactor landed but the UI stayed as one 400×520 dark-only tray popover with PT-BR transliterated strings, per-component hex literals, and a flat 20-item filename list as the only history surface. The sibling project `quick-capture` (`../quick-capture`) ships a polished design language (Inter, violet accent, light+dark via OS pref, sidebar+detail Settings, two-pane Inbox, multi-window architecture with Accessory→Regular activation). This refactor brings `koko-notes-whisper` to the same bar.

Full architectural decisions live in `docs/adr/0002-three-window-architecture-and-quick-capture-design-language.md`. This PRD is the implementation cross-check.

## Non-goals

- No search (substring or otherwise) over transcriptions. Deferred.
- No star / favorite / archive feature on transcriptions.
- No on-disk rewrite of legacy `.md` files. Frontmatter expansion applies only to new sessions.
- No Dock-style 96×96 floating widget. Whisper sessions run minutes, not seconds.
- No auto-updater. `About` replaces the `Updates` section from quick-capture's Settings.
- No new product features beyond `Delete transcription`.

## Architecture (from ADR-0002)

| Window      | Route        | Size / chrome                                                              |
| ----------- | ------------ | -------------------------------------------------------------------------- |
| `main`      | `/`          | 900×600, min 700×450, resizable                                            |
| `recording` | `/recording` | 480×320, frameless, transparent, always_on_top, centered, skip_taskbar     |
| `settings`  | `/settings`  | sidebar+detail, ~840×600, resizable                                        |

- Activation policy: `Accessory` by default, `Regular` when `main` is visible.
- All three windows: close → hide (not destroy).
- `tauri-plugin-window-state` persists bounds.
- Model lifecycle (`check_model_status` / `download_model` / `initialize_whisper` / `check_incomplete_sessions`) moves from `+page.svelte`'s `onMount` to Rust `setup()`. Status surfaced via events.

## Decisions (from /grill-me)

| Branch                       | Decision                                                                   |
| ---------------------------- | -------------------------------------------------------------------------- |
| Scope                        | Visual + window restructure (not just visual polish).                      |
| Window count                 | 3 (main + recording + settings). No Dock.                                  |
| Theme                        | Light default + dark via `prefers-color-scheme`.                           |
| Strings                      | English. PT-BR replaced.                                                   |
| Tokens                       | Clone quick-capture 1:1 (Inter, violet accent, same bg/surface palette).   |
| Main layout                  | Two-pane (40/60 list + detail).                                            |
| Main features                | Read-only browse + delete. Search deferred.                                |
| Recording popover            | 480×320 frameless transparent always-on-top. Auto-hide on complete.        |
| Shortcuts                    | `Cmd+Shift+R` recording, `Cmd+Shift+H` Main, `Cmd+,` Settings.             |
| Settings shape               | Sidebar+detail, 3 groups (Capture / General / Advanced).                   |
| Frontmatter                  | Expanded for new files (`Duracao`, `Microfone`, `Audio do sistema`, `Trechos`). Legacy: hide missing rows. |
| Slice plan                   | 5 tracer-bullet slices (`ui-01` to `ui-05`). Linear chain.                 |
| Model lifecycle              | Rust `setup()`. Events drive splash in each window.                        |
| Recovery banner              | Top of Main pane.                                                          |

## Slices

1. `01-design-tokens-and-theme.md` — `globals.css` + Inter + dark via prefers-color-scheme.
2. `02-three-window-split.md` — Three Tauri windows + tray menu + EN strings + model boot to Rust `setup()`.
3. `03-main-two-pane-history.md` — Main two-pane + Detail pane + frontmatter expansion + parse on read.
4. `04-delete-and-recovery-banner.md` — Delete action + recovery banner in Main.
5. `05-polish.md` — Status bar + empty/error states + keyboard nav.

Each slice is one PR, squash-merged, Conventional Commits with `feat(ui-NN)` or `refactor(ui-NN)` scope. App runs on `main` between slices.
