# koko-notes-whisper — Domain Glossary

Single source of truth for terms used in this project. Implementation lives in code; this file is glossary only. Grows lazily as slices pin down terminology — only nouns and verbs the codebase actually uses appear here.

## Track

An audio source recorded into a session. Closed set with two members today:

| Track        | Source                                      | Backend                           |
| ------------ | ------------------------------------------- | --------------------------------- |
| `Microphone` | The user's mic input                        | CoreAudio (via cpal)              |
| `System`     | System-wide audio output captured for input | ScreenCaptureKit (via cpal fork)  |

Tracks can be independently enabled/disabled in settings. A recording with both tracks enabled produces parallel Chunks per Track.

## Recording Session

The window between the user pressing record (or triggering `Cmd+Shift+R`) and pressing stop. A Session has a start time, a directory on disk, a status (`recording` -> `completed` or `recovered`), and one **active** `TrackSession` per enabled Track.

Two flavors of the Session record exist intentionally:

- **Active session** (in `state::ActiveSession`): the live mutable thing in memory while recording. Mutated by the audio capture thread and the transcription pipeline.
- **Persisted session** (in `session::SessionInfo`): the on-disk record (a manifest JSON in the session directory) the recovery flow loads when the app starts and finds an `recording`-status session left behind.

The active form is the source of truth during recording. The persisted form is the source of truth across app restarts.

## Chunk

One slice of audio inside a Session. Each Chunk is:
- A `.wav` file on disk (`<track>_<NNN>.wav`, zero-padded index from `000`).
- An entry in the Session's manifest with sample rate, device name, and (post-transcription) the transcript fragment.

Chunks are produced on a periodic boundary while a Session is active. A Track can have many Chunks; ordering is by index.

## Transcription

The text output of running a Chunk's WAV through `whisper-rs` against the loaded Model. Two scopes:

- **Per-chunk transcription**: stored in the manifest entry for that Chunk.
- **Per-session transcript**: the aggregate. Built from per-Chunk transcripts at session-completion or recovery time. Single-track sessions render as plain text; multi-track sessions render as `## Microphone\n\n...\n\n## System\n\n...` (markdown headers in fixed Track order).

The aggregate is built by `commands::build_transcript` and is pure — given the per-Track text map, the output is deterministic.

## Model

The whisper.cpp ggml model file used by `whisper-rs`. Today exactly one: `ggml-large-v3-turbo-q5_0.bin`, fetched from Hugging Face on first run and cached under `~/Library/Application Support/koko-notes-whisper/models/`.

Model concerns live in `model/` (download, path resolution, status check). Inference concerns live in `transcription/`.

## Device

A configured audio input source. A Device has a `name` (the OS-reported device label) and a `device_type`:

- `Input` — a CoreAudio input device, used for the `Microphone` Track.
- `System` — a ScreenCaptureKit-backed virtual device, used for the `System` Track.

Devices are listed at runtime by querying cpal. The user's selection per Track is persisted in `AppConfig`.
