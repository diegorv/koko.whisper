use crate::state::{AppState, TrackName};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

pub mod model;
pub mod recording;
pub mod settings;
pub mod transcriptions;

// `toggle_recording_impl` is invoked from `tray::handle_menu_event` and
// `shortcuts::register` via `crate::commands::toggle_recording_impl(...)`,
// so the re-export must live here. The Tauri command functions are NOT
// re-exported because `tauri::generate_handler!` cannot follow re-exports
// — it needs both the fn and the `__cmd__<name>` macro shim at the same
// path. lib.rs references them via the full `commands::<submod>::*` path.
pub use recording::toggle_recording_impl;

use recording::{build_transcript, copy_to_clipboard, save_markdown};

/// Returns (status, elapsed_seconds) for frontend to sync on mount.
#[tauri::command]
pub fn get_app_status(state: State<'_, AppState>) -> Result<(u8, u64), String> {
    let status = state.app_status.load(Ordering::Relaxed);
    let elapsed = state
        .recording_started_at
        .lock()
        .map_err(|e| e.to_string())?
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);
    Ok((status, elapsed))
}

// --- Crash Recovery ---

#[derive(serde::Serialize, Clone)]
pub struct IncompleteSession {
    pub session_id: String,
    pub started_at: String,
    pub total_chunks: u32,
    pub transcribed_chunks: u32,
    pub session_dir: String,
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
            let total_chunks = manifest.chunks.len() as u32;
            let transcribed_chunks = manifest
                .chunks
                .iter()
                .filter(|c| c.transcript.is_some())
                .count() as u32;

            result.push(IncompleteSession {
                session_id: manifest.session_id,
                started_at: manifest.started_at,
                total_chunks,
                transcribed_chunks,
                session_dir: session_dir.to_string_lossy().to_string(),
            });
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
                let ctx = ctx_guard.as_ref().ok_or("Whisper nao inicializado")?;
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
    #[test]
    fn test_silence_detection_rms() {
        // Silent audio (all zeros) → below threshold
        let silent = vec![0.0f32; 16000];
        let rms = (silent.iter().map(|s| s * s).sum::<f32>() / silent.len() as f32).sqrt();
        assert!(rms < 0.01);

        // Loud audio (sine wave) → above threshold
        let loud: Vec<f32> = (0..16000)
            .map(|i| (i as f32 / 16000.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect();
        let rms = (loud.iter().map(|s| s * s).sum::<f32>() / loud.len() as f32).sqrt();
        assert!(rms >= 0.01);

        // Very quiet audio → below threshold
        let quiet: Vec<f32> = vec![0.001; 16000];
        let rms = (quiet.iter().map(|s| s * s).sum::<f32>() / quiet.len() as f32).sqrt();
        assert!(rms < 0.01);
    }
}
