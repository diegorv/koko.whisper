# Refactor PRD — PoC to structured layout, mirroring quick-capture

Status: ready-for-agent
Owner: @diegorv
Reference: /grill-me session, 2026-05-22

## Why

`koko-notes-whisper` is functional today but was built as a flat PoC. The companion project `quick-capture` (sibling repo at `../quick-capture`) has matured into a per-domain layout with colocated tests, ADRs, CONTEXT.md, vitest, and a stricter CI gate. Goal: bring this repo to the same structural bar, without changing user-visible behavior.

## Non-goals

- No new product features.
- No auto-updater (`tauri-plugin-updater`), no nightly channel. Build-info display only.
- No persistence backwards compatibility — refactor is free to break on-disk format. Old config files logged and ignored.
- No `/transcriptions` route. Recording + list stay on home.
- No workflow_call / nightly.yml / run-all.yml CI infrastructure.

## Decisions (from /grill-me)

| Branch | Decision |
| --- | --- |
| Refactor shape | Slice-by-slice. App runnable on `main` throughout. |
| Testing bar | Pragmatic. Extract pure core + unit test. I/O shell smoke-tested. |
| Domain map | `audio/{devices,resample,stream}`, `model/`, `transcription/`, `session/`, `config/`, `state/`, `tray/`, `shortcuts/`, `commands/{recording,transcriptions,settings,model,session}`. `state` and `session` stay separate (runtime vs persisted). |
| Slice order | Bottom-up (leaves first), frontend last. 18 slices. |
| Docs | Hybrid. Slice 0 ships small `CONTEXT.md` (Track/Session/Chunk/Transcription/Model/Device) + ADR-0001. Other ADRs land per-slice when warranted. |
| Updater / channels | Build-info display only. `__BUILD_INFO__` + git sha in `/settings`. No auto-update plugin. |
| Routes | Two. `/` = recording + list. `/settings` = settings page. |
| Persistence compat | Wipe-and-restart. No format compatibility commitments. Old config = log + ignore + start fresh. |
| CI scope | Testable subset of quick-capture: `pnpm vitest run` + `pnpm build` + `ci-success` aggregate + composite `.github/actions/setup/action.yml`. Skip workflow_call / nightly / run-all. |
| Branch strategy | Feature branch per slice. PR-gated by `ci-success`. |
| Commit format | `refactor(slice-NN):` / `test(slice-NN):` / `chore(slice-NN):` / `docs(slice-NN):` / `feat(slice-NN):`. |

## Slice list

```
 0 chore(slice-00): scaffold — vitest, CI subset, CONTEXT.md, ADR-0001
 1 refactor(slice-01): extract model/ (+ tests: path resolution, URL const)
 2 refactor(slice-02): extract config/ (+ tests: serde round-trip, defaults)
 3 refactor(slice-03): extract session/ persistence (+ tests: manifest IO, recovery detect)
 4 refactor(slice-04): split audio/{devices,resample} pure cores (+ tests: mono mix, resample to 16k)
 5 refactor(slice-05): extract audio/stream (smoke note in PR — cpal real-time thread)
 6 refactor(slice-06): extract transcription/ (+ tests: params, prompt builder)
 7 refactor(slice-07): extract state/ (+ tests: status enum transitions)
 8 refactor(slice-08): extract tray/ (+ tests: TrayInfo -> title string)
 9 refactor(slice-09): extract shortcuts/ (smoke note in PR — Tauri global shortcut glue)
10 refactor(slice-10): commands/recording.rs (+ tests: build_transcript)
11 refactor(slice-11): commands/transcriptions.rs (+ tests: filename parse + sort)
12 refactor(slice-12): commands/settings.rs (smoke only — thin glue)
13 refactor(slice-13): commands/model.rs (smoke only — thin glue)
14 refactor(slice-14): commands/session.rs (+ tests: recovery candidate detection)
15 refactor(slice-15): lib.rs cleanup — Tauri builder wiring only
16 feat(slice-16): build-info injection + version display in /settings
17 refactor(slice-17): frontend — split lib/{recording,transcriptions,settings} + /settings route
```

## Acceptance per slice

- Branch `refactor/slice-NN-<short-name>` created.
- Files moved per slice scope.
- Pure functions unit-tested where present. Slices with no pure surface include a smoke-test note in the PR body.
- `pnpm check && pnpm vitest run && pnpm build && cargo test --manifest-path src-tauri/Cargo.toml` all green locally before push.
- CI green on PR. `ci-success` aggregate passes.
- App still launches and records on macOS (manual smoke).
- PR merged with squash; commit message follows `<type>(slice-NN): <subject>`.

## Open questions deferred

- Branch protection on `main` to make `ci-success` a required check — out of refactor scope (GitHub Settings change).
- Cargo workspace / multi-crate layout — not adopted. Quick-capture is single-crate too; this is fine.
- `arboard` (clipboard) usage in current whisper — usage site to be located during slice 10 or 11.

## Status notes

Created: 2026-05-22. Source of truth lives here until refactor completes; each slice's PR body links back to this file.
