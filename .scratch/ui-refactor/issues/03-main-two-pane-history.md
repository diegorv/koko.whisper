# ui-03: Main two-pane history + Detail pane + frontmatter expansion

Status: ready-for-agent
Reference: ADR-0002 §4, §9, PRD `.scratch/ui-refactor/PRD.md`

## What to build

Replace the placeholder Main shell with a two-pane history browser: a 40/60 split where the left column lists past transcription sessions (newest first) and the right pane shows the selected session's full transcript with metadata and per-session actions. Expand the markdown frontmatter so the metadata block has real data for new sessions, and let legacy files degrade gracefully.

### List pane (40%)

Each row shows:
- Timestamp (date + time, formatted for the current locale)
- Duration (mm:ss or h:mm:ss), if available in frontmatter
- First line of the transcript body as a preview (truncate to ~80 chars)

Order: newest first by file mtime / frontmatter date. No pagination for this slice — return all `.md` entries.

### Detail pane (60%)

Three blocks, top to bottom:

1. **Header**: large date/time + duration. Action buttons on the right: `Copy` (copies full transcript to clipboard), `Reveal in Finder` (existing behaviour), `Delete` (placeholder for ui-04 — disabled or hidden in this slice).
2. **Meta**: uppercase muted labels with values, two-column grid. Rows: Date, Duration, Language, Microphone, System audio, Chunks. Any row whose source field is absent in the file's frontmatter is **omitted entirely** (no "—" placeholder). Legacy files (only `Data` + `Idioma`) end up showing only Date + Language rows.
3. **Body**: full transcript. If the file body contains `## Microphone` / `## System` headers (multi-track), render each section with a chip-style label ("Eu" / "Participante" or equivalent — keep whatever wording exists today, but in English: "You" / "Other"). Single-track renders as plain prose.

Empty selection state: detail pane shows a centered "Select a transcription to read" message.

### Backend changes

1. `recording::save_markdown` writes additional frontmatter for new sessions:
   ```
   # Voice transcription

   **Date:** YYYY-MM-DD HH:MM:SS
   **Duration:** mm:ss
   **Language:** Portuguese (BR)
   **Microphone:** <device name or "Disabled">
   **System audio:** <device name or "Disabled">
   **Chunks:** <N>

   ---

   <body>
   ```
2. `get_transcriptions` is amended to return parsed metadata alongside the existing preview + path. Suggested shape:
   ```rust
   TranscriptionEntry {
     filename: String,
     path: String,
     preview: String,
     date: Option<String>,
     duration_seconds: Option<u32>,
     language: Option<String>,
     mic_device: Option<String>,
     sys_device: Option<String>,
     chunks: Option<u32>,
   }
   ```
   The command now returns ALL `.md` files in the folder, not the first 20. A new command `get_transcription_body(path)` (or extending `get_transcriptions` to include body) lets the Detail pane read the full transcript on selection.
3. Existing files (`Data` / `Idioma` only) are not rewritten. The parser tolerates missing fields and returns `None` for each.

### Frontend changes

- `TranscriptionList.svelte` is rewritten or replaced — it is now the list column inside Main, not the standalone block. Selection state lives in the page.
- A new `TranscriptionDetail.svelte` renders the detail pane.
- The Main `/` route composes `<List>` + `<Detail>` inside the 40/60 grid.

## Acceptance criteria

- [ ] Main `/` route renders a 40/60 two-pane layout
- [ ] List shows all `.md` files in the output folder, newest first, with timestamp + duration + preview
- [ ] Clicking a row populates the Detail pane
- [ ] Detail Header shows date + duration + `Copy` / `Reveal in Finder` buttons (Delete present but disabled/hidden in this slice)
- [ ] Detail Meta omits rows whose frontmatter field is absent — legacy files show only Date + Language
- [ ] Detail Body parses `## Microphone` / `## System` headers into chip-labeled sections; single-track renders plain
- [ ] New recordings save frontmatter with Date, Duration, Language, Microphone, System audio, Chunks
- [ ] Old `.md` files remain untouched on disk
- [ ] Empty list state: list shows "No transcriptions yet" (polish in ui-05 — minimal here)
- [ ] No selection: detail shows "Select a transcription to read"
- [ ] `pnpm vitest run` and `pnpm build` pass; Rust tests pass

## Blocked by

ui-02 (the Main window must exist with its `/` route).
