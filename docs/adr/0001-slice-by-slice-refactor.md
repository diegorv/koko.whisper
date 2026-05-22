# ADR-0001: Slice-by-slice refactor from PoC layout to per-domain layout

Status: Accepted
Date: 2026-05-22

## Context

This repo began as a working proof-of-concept with a flat layout: 9 Rust files at `src-tauri/src/`, 3 Svelte components at `src/lib/`, one route, no tests, no domain documentation. The companion project `quick-capture` (sibling repo) matured into a per-domain layout with colocated tests, ADRs, a domain glossary, vitest, and a stricter CI gate. The intent of this refactor is to bring this repo to the same structural bar — preserving all behavior and on-disk artifacts where reasonable, but restructuring code so future work is testable and AI-navigable.

A full plan with the 18-slice list and per-slice scope lives at `.scratch/refactor/PRD.md`.

## Decision

The refactor runs as a sequence of **slices**. The decisions below are load-bearing across every slice; this ADR exists so each slice's PR can cite it rather than relitigate them.

### 1. Slice-by-slice, not big-bang

The app stays runnable on `main` throughout. Each slice is one PR, one merge, one conventional commit. Slices are bottom-up: leaves (pure, no dependents) before roots (touch everything). Frontend last.

### 2. Pragmatic testing bar (pure-function only)

Each slice extracts a pure core where one exists and unit-tests it. I/O-heavy code (cpal stream callback, whisper-rs FFI, Tauri commands, tray/shortcut glue) is smoke-tested manually; the slice's PR body states the manual steps. Mock-heavy unit tests against I/O boundaries are not pursued.

### 3. Persistence backwards compatibility is not maintained

The refactor is free to break on-disk format compatibility for `config.json` and session manifests. Old config files encountered at startup are logged and ignored; the app starts fresh with defaults. Pre-existing WAV files remain on disk as files but become orphans to the new session index.

This is a deliberate scope cut to keep the refactor structural-only. A future ADR (post-refactor) can introduce a real migration discipline if needed.

### 4. Build-info display only — no auto-updater in scope

Whisper does not adopt `tauri-plugin-updater`, `tauri-plugin-process`, `tauri-plugin-store`, nightly channels, or signed-release CI infrastructure as part of this refactor. The refactor does adopt build-info injection (git short SHA + version visible in `/settings`) as one dedicated slice (`feat(slice-16):`). Auto-updater is a future feature, not refactor work.

### 5. Two routes — `/` and `/settings`

The frontend split lands `/settings` as its own route; the recording controls and the transcription list stay together on `/`. A `/transcriptions` route is not in scope.

### 6. CI gates each slice via `ci-success` aggregate

The slice-0 CI rewrite adds `pnpm vitest run`, `pnpm build`, a composite `.github/actions/setup/action.yml`, and a `ci-success` aggregate gate. The aggregate is the single status check intended for branch protection (jobs skipped by the paths filter still pass through the aggregate). Branch protection itself (making `ci-success` *required*) is a one-time GitHub Settings change outside this refactor's scope.

### 7. Feature branch per slice, Conventional Commits with slice scope

Each slice is a branch `refactor/slice-NN-<short-name>` opened as a PR against `main`. Commit subjects use Conventional Commits with the slice as the scope: `refactor(slice-04): split audio/{devices,resample} pure cores`. Types in active use: `refactor`, `test`, `chore`, `docs`, `feat`. PRs are squash-merged with the commit subject preserved.

## Consequences

- Each slice is reviewable and revertable independently. A bad slice does not block subsequent slices once reverted.
- The 18-slice list will run for several development sessions. The PRD at `.scratch/refactor/PRD.md` is the cross-session checkpoint.
- Tests added per slice are real (against pure cores) rather than mocked-against-Tauri. Smoke notes in PR bodies are the documented exception, not the default.
- Users who run a dev build mid-refactor may lose their `config.json` if a config-touching slice has shipped and they had not re-set their preferences. Acceptable given solo dev use today.
- Branch-protection enforcement of `ci-success` is opt-in and not enforced by this ADR. CI still runs on every PR regardless.
