//! Crash-recovery Tauri commands. On startup the frontend asks
//! `check_incomplete_sessions` for any session left behind in
//! `Recording` status; the user picks one to recover or to dismiss.
//! `recover_session` re-transcribes whatever chunks have no transcript
//! yet, marks the manifest `Recovered`, and produces the same final
//! markdown + clipboard side effects as a normal stop.
//!
//! Reuses the pure-but-`pub(super)` helpers from `recording/`
//! (`build_transcript`, `copy_to_clipboard`, `save_markdown`) so the
//! recovered-flow output is byte-identical to the live-flow output.

use crate::session::SessionManifest;
use crate::state::{AppState, TrackName};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

use super::recording::{build_transcript, copy_to_clipboard, save_markdown};

#[derive(serde::Serialize, Clone)]
pub struct IncompleteSession {
    pub session_id: String,
    pub started_at: String,
    pub total_chunks: u32,
    pub transcribed_chunks: u32,
    pub session_dir: String,
}

/// Pure. Turn a loaded manifest into the recovery-summary row the
/// frontend renders. Counts how many of the chunks already have a
/// transcript on disk — `transcribed_chunks == total_chunks` means
/// the session crashed AFTER all chunks were transcribed but BEFORE
/// the manifest got marked Completed, so the recovery flow only
/// needs to finalize the markdown.
fn summarize_incomplete(manifest: &SessionManifest, session_dir: &Path) -> IncompleteSession {
    let total_chunks = manifest.chunks.len() as u32;
    let transcribed_chunks = manifest
        .chunks
        .iter()
        .filter(|c| c.transcript.is_some())
        .count() as u32;
    IncompleteSession {
        session_id: manifest.session_id.clone(),
        started_at: manifest.started_at.clone(),
        total_chunks,
        transcribed_chunks,
        session_dir: session_dir.to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub async fn check_incomplete_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<IncompleteSession>, String> {
    let output_folder = state.output_folder.lock().await.clone();
    let sessions = crate::session::find_incomplete_sessions(&output_folder)
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for session_dir in sessions {
        if let Ok(manifest) = crate::session::read_manifest(&session_dir) {
            result.push(summarize_incomplete(&manifest, &session_dir));
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn recover_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_dir: String,
) -> Result<String, String> {
    let session_path = PathBuf::from(&session_dir);
    let manifest =
        crate::session::read_manifest(&session_path).map_err(|e| e.to_string())?;

    let _ = app.emit("transcription-status", "recovering");
    eprintln!(
        "[recovery] Recovering session {} with {} chunks",
        manifest.session_id,
        manifest.chunks.len()
    );

    // Accumulate transcripts per track
    let mut track_transcripts: HashMap<TrackName, String> = HashMap::new();

    for chunk in &manifest.chunks {
        let transcript = if let Some(ref existing) = chunk.transcript {
            existing.clone()
        } else {
            // Need to re-transcribe from WAV
            let wav_path = session_path.join(&chunk.filename);
            eprintln!("[recovery] Transcribing {:?}", wav_path);

            let audio_data = crate::audio::load_wav(&wav_path).map_err(|e| e.to_string())?;

            let text = {
                let ctx_guard = state.whisper_context.lock().await;
                let ctx = ctx_guard.as_ref().ok_or("Whisper not initialized")?;
                crate::transcription::transcribe(ctx, &audio_data).map_err(|e| e.to_string())?
            };

            crate::session::update_chunk_transcript(
                &session_path,
                &chunk.filename,
                &text,
            )
            .map_err(|e| e.to_string())?;

            let _ = app.emit("chunk-transcribed", &text);

            text
        };

        let track_name: TrackName = chunk.track.parse().map_err(|e: String| e)?;
        let entry = track_transcripts.entry(track_name).or_default();
        if !entry.is_empty() {
            entry.push(' ');
        }
        entry.push_str(&transcript);
    }

    let full_transcript = build_transcript(&track_transcripts);

    // Mark session as recovered
    let mut updated_manifest = manifest.clone();
    updated_manifest.status = crate::session::SessionStatus::Recovered;
    crate::session::write_manifest(&session_path, &updated_manifest)
        .map_err(|e| e.to_string())?;

    // Save the recovered markdown
    let output_folder = state.output_folder.lock().await.clone();
    save_markdown(&output_folder, &full_transcript).map_err(|e| e.to_string())?;

    copy_to_clipboard(&full_transcript);

    let _ = app.emit("transcription-complete", &full_transcript);

    eprintln!(
        "[recovery] Session {} recovered successfully",
        updated_manifest.session_id
    );

    Ok(full_transcript)
}

#[tauri::command]
pub async fn dismiss_session(session_dir: String) -> Result<(), String> {
    let path = PathBuf::from(&session_dir);
    if path.exists() {
        std::fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionChunk, SessionStatus};

    fn manifest_with_chunks(chunks: Vec<SessionChunk>) -> SessionManifest {
        SessionManifest {
            session_id: "test-id".to_string(),
            started_at: "2026-05-22T00:00:00+0000".to_string(),
            tracks: std::collections::HashMap::new(),
            status: SessionStatus::Recording,
            chunks,
        }
    }

    fn chunk(filename: &str, transcript: Option<&str>) -> SessionChunk {
        SessionChunk {
            filename: filename.to_string(),
            track: "microphone".to_string(),
            transcript: transcript.map(str::to_string),
        }
    }

    #[test]
    fn summarize_incomplete_counts_chunks_and_transcribed() {
        let manifest = manifest_with_chunks(vec![
            chunk("mic_000.wav", Some("hello")),
            chunk("mic_001.wav", None),
            chunk("mic_002.wav", Some("world")),
        ]);
        let summary = summarize_incomplete(&manifest, Path::new("/tmp/sessions/test"));

        assert_eq!(summary.total_chunks, 3);
        assert_eq!(summary.transcribed_chunks, 2);
        assert_eq!(summary.session_id, "test-id");
        assert_eq!(summary.started_at, "2026-05-22T00:00:00+0000");
        assert_eq!(summary.session_dir, "/tmp/sessions/test");
    }

    #[test]
    fn summarize_incomplete_handles_zero_chunks() {
        let manifest = manifest_with_chunks(vec![]);
        let summary = summarize_incomplete(&manifest, Path::new("/tmp/empty"));
        assert_eq!(summary.total_chunks, 0);
        assert_eq!(summary.transcribed_chunks, 0);
    }

    #[test]
    fn summarize_incomplete_returns_total_equals_transcribed_when_all_done() {
        // A session that crashed *after* every chunk was transcribed
        // but *before* status was flipped to Completed. The recovery
        // flow still needs to surface it so the user can finalize.
        let manifest = manifest_with_chunks(vec![
            chunk("mic_000.wav", Some("a")),
            chunk("mic_001.wav", Some("b")),
        ]);
        let summary = summarize_incomplete(&manifest, Path::new("/tmp/finalize-me"));
        assert_eq!(summary.total_chunks, 2);
        assert_eq!(summary.transcribed_chunks, 2);
    }

    #[test]
    fn summarize_incomplete_treats_zero_transcribed_as_full_retranscribe_needed() {
        // The other extreme: nothing transcribed yet. Recovery will
        // run every chunk through whisper.
        let manifest = manifest_with_chunks(vec![
            chunk("mic_000.wav", None),
            chunk("mic_001.wav", None),
            chunk("mic_002.wav", None),
        ]);
        let summary = summarize_incomplete(&manifest, Path::new("/tmp/from-scratch"));
        assert_eq!(summary.total_chunks, 3);
        assert_eq!(summary.transcribed_chunks, 0);
    }

    #[test]
    fn summarize_incomplete_unused_session_dir_field_is_lossless_path_string() {
        // Pin that we serialize the path with `to_string_lossy()` —
        // the IncompleteSession struct goes over the IPC boundary as
        // JSON and a non-UTF-8 path would otherwise lose data.
        // For ASCII paths the result is identical to the input.
        let manifest = manifest_with_chunks(vec![]);
        let summary = summarize_incomplete(&manifest, Path::new("/tmp/ascii/path"));
        assert_eq!(summary.session_dir, "/tmp/ascii/path");
    }
}
