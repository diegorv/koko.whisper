# ui-01: Design tokens + Inter + dark via prefers-color-scheme

Status: ready-for-agent
Reference: ADR-0002 §3, PRD `.scratch/ui-refactor/PRD.md`

## What to build

Introduce a shared design-token layer mirroring `quick-capture`'s palette and typography, and switch the app from dark-only hardcoded styles to light-default-with-OS-dark-override. Existing components keep their structure — they just stop carrying per-component hex literals and start reading from CSS custom properties.

Token surface (declared once, consumed everywhere):

- Font stack: `Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`
- Light palette: bg `#f6f6f6`, surface `#ffffff`, text `#0f0f0f`, muted text ~55% black, accent `rgba(76, 29, 149, 1)`, border ~8-12% black
- Dark palette (under `@media (prefers-color-scheme: dark)`): bg `#1c1c1c`, surface `#232327`, text `#f6f6f6`, muted text ~55% white, accent `rgba(167, 139, 250, 1)`, border ~8-12% white
- Spacing scale and radii consistent with quick-capture's component CSS

Apply the tokens to the current `+page.svelte`, `RecordingView.svelte`, `TranscriptionList.svelte`, and `Settings.svelte` so the app looks visually consistent on both themes after this slice — strings stay PT-BR, structure stays as it is. Semantic colors that are not tokens (recording-red `#ff4444`, VU-meter green/orange/red) stay inline.

## Acceptance criteria

- [ ] One `globals.css` (or equivalent root-level stylesheet) declares CSS custom properties for color, font, spacing, and radius tokens
- [ ] A `@media (prefers-color-scheme: dark)` block in the same file overrides only the color tokens; structure tokens are unchanged
- [ ] Every existing Svelte component reads colors / font / spacing from tokens — no inline hex literals remain except the documented semantic colors (record red, VU meter colors)
- [ ] Running the app on macOS with system "Light" set renders the light palette; flipping system to "Dark" flips the app without restart
- [ ] `pnpm vitest run` and `pnpm build` pass

## Blocked by

None — can start immediately.
