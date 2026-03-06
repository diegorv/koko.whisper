# Koko Whisper

[![CI](https://github.com/diegorv/koko.whisper/actions/workflows/ci.yml/badge.svg)](https://github.com/diegorv/koko.whisper/actions/workflows/ci.yml) [![Security](https://github.com/diegorv/koko.whisper/actions/workflows/security.yml/badge.svg)](https://github.com/diegorv/koko.whisper/actions/workflows/security.yml) [![Release](https://github.com/diegorv/koko.whisper/actions/workflows/release.yml/badge.svg)](https://github.com/diegorv/koko.whisper/actions/workflows/release.yml) [![Privacy Check](https://github.com/diegorv/koko.whisper/actions/workflows/privacy.yml/badge.svg)](https://github.com/diegorv/koko.whisper/actions/workflows/privacy.yml)

A personal desktop voice transcription app powered by [Whisper](https://github.com/openai/whisper), built with Svelte 5 and Tauri 2.

All transcription runs locally on your Mac using the Whisper large-v3-turbo model with Metal GPU acceleration — no cloud, no API calls, privacy first. Built 100% with [Claude Code](https://docs.anthropic.com/en/docs/claude-code) with human review.

> [!CAUTION]
> **EARLY STAGE SOFTWARE — DO NOT USE FOR ANYTHING IMPORTANT**
>
> This project is in a very early stage of development and is **not recommended for use by anyone**.
> There is a real risk of **loss of recordings and transcriptions** made by the program. Do not rely on this software to preserve any important data.
>
> Expect breaking changes, missing features, and rough edges.
> **macOS only** — built exclusively for macOS (Apple Silicon) with no plans to support other platforms.

> [!WARNING]
> **We are not accepting pull requests, issues, or external contributions at this time.**

## Features

- **Local transcription** — Whisper large-v3-turbo (GGML, quantized Q5_0) via Metal GPU, ~547MB model
- **Dual-track recording** — microphone + system audio (ScreenCaptureKit) with independent controls
- **Global shortcut** — `Cmd+Shift+R` to start/stop recording from anywhere
- **System tray** — lives in the menu bar with live recording timer, no dock icon
- **Chunked processing** — 5-minute intervals with partial transcripts during recording
- **Session recovery** — automatic crash recovery with WAV chunks and manifest files
- **Clipboard integration** — transcription results copied to clipboard automatically
- **Audio device selection** — choose specific mic and system audio devices with live VU meters
- **Configurable output** — transcriptions saved as text files to a folder of your choice

## Stack

| Layer | Technology |
|-------|-----------|
| Frontend | Svelte 5 (runes), SvelteKit, TypeScript |
| Backend | Tauri 2 (Rust) |
| Transcription | whisper-rs (whisper.cpp bindings) with Metal GPU |
| Audio capture | cpal (ScreenCaptureKit fork for system audio) |
| Audio processing | rubato (resampling to 16kHz), hound (WAV I/O) |
| Model download | reqwest (one-time download from HuggingFace) |
| Package manager | pnpm |

## Getting Started

### Prerequisites

- macOS 13+ (Ventura or later) on Apple Silicon
- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 22+
- [pnpm](https://pnpm.io/) 10+

### Quick Start

```bash
# 1. Install frontend dependencies
pnpm install

# 2. Run in dev mode
pnpm tauri dev
```

On first launch, the app will automatically download the Whisper model (~547MB) from HuggingFace.

### Commands

```bash
pnpm tauri dev            # Run app in dev mode (frontend + Tauri)
pnpm dev                  # Run frontend only (no Tauri window)
pnpm build                # Build frontend for production
pnpm tauri build          # Build the full desktop app
pnpm check                # TypeScript type checking
cargo test --manifest-path src-tauri/Cargo.toml   # Run Rust tests
```

## Project Structure

```
src/
  routes/
    +page.svelte            # Main page: model download, init, view routing
  lib/
    RecordingView.svelte    # Recording UI: button, timer, partial transcripts
    Settings.svelte         # Device selection, output folder, VU meters
    TranscriptionList.svelte # Recent transcriptions list

src-tauri/src/
  lib.rs                   # App setup: plugins, shortcuts, tray, audio capture loop
  commands.rs              # Tauri command handlers (record, stop, settings, devices)
  audio.rs                 # Audio capture (cpal), resampling (rubato), WAV I/O (hound)
  transcription.rs         # Whisper transcription with hallucination filtering
  model.rs                 # Model download from HuggingFace (one-time)
  state.rs                 # App state: tracks, buffers, session, config
  session.rs               # Session management: manifest, recovery, crash handling
  config.rs                # App config: devices, output folder, persistence
  tray.rs                  # System tray: menu, recording timer, status updates
```

## IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Privacy

**Privacy is a core value of this project.** Koko Whisper is designed to work entirely offline — your audio and transcriptions never leave your machine.

- All audio is captured and processed locally
- Transcription runs on-device via Whisper large-v3-turbo with Metal GPU acceleration
- Transcriptions are saved as plain text files to a local folder
- Audio chunks (WAV) are stored locally for session recovery
- **No analytics, no tracking, no accounts, no sign-up**

The only external network call in the entire codebase is:

| Call | Where | Why |
|------|-------|-----|
| HuggingFace model download | `src-tauri/src/model.rs` | One-time download of the Whisper GGML model (~547MB). After download, everything runs offline. |

A [Privacy Check](https://github.com/diegorv/koko.whisper/actions/workflows/privacy.yml) workflow runs on every push and pull request, scanning all `.ts` and `.rs` source files for external network calls. Any new external call that is not explicitly approved will fail the build.
