//! Recording lifecycle Tauri commands. start / stop / toggle, plus
//! the audio-level meter polled by the frontend and the
//! `build_transcript` aggregator that produces the final markdown
//! body once a session ends.
//!
//! `*_impl` variants of start/stop are the actual logic; the
//! `#[tauri::command]` wrappers above them exist only to satisfy
//! Tauri's IPC signature requirements (owned `AppHandle` instead of
//! `&AppHandle`). They are called by the same name from
//! `tray::handle_menu_event` and `shortcuts::register` through the
//! re-exported path `crate::commands::toggle_recording_impl`.

use crate::state::{
    ActiveSession, ActiveTrackSession, AppState, TrackName, STATUS_IDLE, STATUS_RECORDING,
    STATUS_TRANSCRIBING,
};
use chrono::Local;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

/// Build final transcript from per-track transcripts.
/// Single track: plain text. Multiple tracks: markdown headers.
pub fn build_transcript(track_transcripts: &HashMap<TrackName, String>) -> String {
    let non_empty: Vec<_> = track_transcripts
        .iter()
        .filter(|(_, t)| !t.is_empty())
        .collect();

    if non_empty.len() <= 1 {
        return non_empty
            .into_iter()
            .map(|(_, t)| t.clone())
            .next()
            .unwrap_or_default();
    }

    // Multiple tracks: use headers in defined order
    let ordered = [TrackName::Microphone, TrackName::System];
    let mut parts = Vec::new();
    for track in &ordered {
        if let Some(text) = track_transcripts.get(track) {
            if !text.is_empty() {
                parts.push(format!("## {}\n\n{}", track.display_label(), text));
            }
        }
    }
    parts.join("\n\n")
}

pub(super) fn copy_to_clipboard(text: &str) {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            if let Err(e) = clipboard.set_text(text) {
                eprintln!("[clipboard] Failed to copy: {}", e);
            }
        }
        Err(e) => {
            eprintln!("[clipboard] Failed to access clipboard: {}", e);
        }
    }
}

pub(super) fn save_markdown(output_folder: &PathBuf, transcript: &str) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(output_folder)?;
    let now = Local::now();
    let filename = format!("{}.md", now.format("%Y-%m-%d_%H-%M-%S"));
    let file_path = output_folder.join(&filename);

    let content = format!(
        "# Transcricao de Voz\n\n**Data:** {}\n**Idioma:** Portugues (BR)\n\n---\n\n{}\n",
        now.format("%Y-%m-%d %H:%M:%S"),
        transcript
    );

    std::fs::write(&file_path, content)?;
    log::info!("Saved transcription to {:?}", file_path);
    Ok(file_path)
}

#[tauri::command]
pub async fn get_audio_levels(
    state: State<'_, AppState>,
) -> Result<HashMap<String, f32>, String> {
    let mut levels = HashMap::new();
    for (name, track) in &state.tracks {
        let bits = track.peak_level.swap(0, Ordering::Relaxed);
        let level = f32::from_bits(bits).clamp(0.0, 1.0);
        levels.insert(name.to_string(), level);
    }
    Ok(levels)
}

/// Core recording start logic — callable from tray, shortcut, or frontend command.
pub async fn start_recording_impl(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    if state.is_recording.load(Ordering::Relaxed) {
        return Err("Already recording".to_string());
    }

    // Clear all track buffers
    for track in state.tracks.values() {
        let mut buffer = track.buffer.lock().await;
        buffer.clear();
    }

    let output_folder = state.output_folder.lock().await.clone();
    let session_id = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();

    // Build tracks info: only include tracks that have a device (or use default)
    let mut session_tracks = HashMap::new();
    let mut active_tracks = HashMap::new();

    for (name, track) in &state.tracks {
        let device = track.device.lock().await;
        let is_enabled = track.enabled.load(Ordering::Relaxed);
        let has_device = device.is_some() || track.use_default_when_none;

        if has_device && is_enabled {
            let sample_rate = *track.sample_rate.lock().await;
            let device_name = device
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "Default".to_string());

            session_tracks.insert(
                name.to_string(),
                crate::session::TrackInfo {
                    sample_rate,
                    device_name,
                },
            );

            active_tracks.insert(
                *name,
                ActiveTrackSession {
                    chunk_index: 0,
                    accumulated_transcript: String::new(),
                },
            );
        }
    }

    let session_dir =
        crate::session::create_session(&output_folder, &session_id, session_tracks)
            .map_err(|e| e.to_string())?;

    eprintln!(
        "[session] Created session: {} at {:?} with {} track(s)",
        session_id,
        session_dir,
        active_tracks.len()
    );

    {
        let mut active = state.active_session.lock().await;
        *active = Some(ActiveSession {
            session_dir,
            tracks: active_tracks,
        });
    }

    state.is_recording.store(true, Ordering::Relaxed);
    state.app_status.store(STATUS_RECORDING, Ordering::Relaxed);
    *state.recording_started_at.lock().unwrap() = Some(std::time::Instant::now());
    crate::tray::update_tray_menu(app);

    // Notify frontend (if window is open) so it can sync UI
    let _ = app.emit("recording-started", ());

    Ok(())
}

#[tauri::command]
pub async fn start_recording(app: AppHandle) -> Result<(), String> {
    start_recording_impl(&app).await
}

/// Core recording stop logic — callable from tray, shortcut, or frontend command.
pub async fn stop_recording_impl(app: &AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();

    if !state.is_recording.load(Ordering::Relaxed) {
        return Err("Not recording".to_string());
    }

    state.is_recording.store(false, Ordering::Relaxed);
    state
        .app_status
        .store(STATUS_TRANSCRIBING, Ordering::Relaxed);
    *state.recording_started_at.lock().unwrap() = None;
    crate::tray::update_tray_menu(app);

    // Small delay to let final audio chunks arrive
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let _ = app.emit("transcription-status", "processing");

    // Process final chunks for all active tracks
    let track_names: Vec<TrackName> = {
        let session_guard = state.active_session.lock().await;
        session_guard
            .as_ref()
            .map(|s| s.tracks.keys().copied().collect())
            .unwrap_or_default()
    };

    for track_name in &track_names {
        match crate::process_track_chunk(app, *track_name).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "[session] Final chunk {}: {} (may be empty)",
                    track_name, e
                );
            }
        }
    }

    // Gather transcripts per track
    let (transcripts, session_dir) = {
        let mut session_guard = state.active_session.lock().await;
        let session = session_guard.take().ok_or("No active session")?;
        let transcripts: HashMap<TrackName, String> = session
            .tracks
            .into_iter()
            .map(|(name, ts)| (name, ts.accumulated_transcript))
            .collect();
        (transcripts, session.session_dir)
    };

    let full_transcript = build_transcript(&transcripts);

    if full_transcript.is_empty() {
        state.app_status.store(STATUS_IDLE, Ordering::Relaxed);
        crate::tray::update_tray_menu(app);
        return Err("Nenhum audio gravado".to_string());
    }

    // Mark session as completed
    crate::session::complete_session(&session_dir).map_err(|e| e.to_string())?;

    eprintln!(
        "[session] Completed, transcript length: {} chars",
        full_transcript.len()
    );

    // Save final markdown
    let output_folder = state.output_folder.lock().await.clone();
    save_markdown(&output_folder, &full_transcript).map_err(|e| e.to_string())?;

    // Copy to clipboard
    copy_to_clipboard(&full_transcript);

    let _ = app.emit("transcription-complete", &full_transcript);

    state.app_status.store(STATUS_IDLE, Ordering::Relaxed);
    crate::tray::update_tray_menu(app);

    Ok(full_transcript)
}

/// Toggle recording on/off — used by tray menu and global shortcut.
pub async fn toggle_recording_impl(app: &AppHandle) {
    let state = app.state::<AppState>();
    let status = state.app_status.load(Ordering::Relaxed);
    if status == STATUS_TRANSCRIBING {
        return;
    }
    if state.is_recording.load(Ordering::Relaxed) {
        let _ = stop_recording_impl(app).await;
    } else {
        let _ = start_recording_impl(app).await;
    }
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<String, String> {
    stop_recording_impl(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_transcript_empty_map_returns_empty_string() {
        let transcripts: HashMap<TrackName, String> = HashMap::new();
        assert_eq!(build_transcript(&transcripts), "");
    }

    #[test]
    fn build_transcript_single_track_returns_plain_text() {
        let mut t = HashMap::new();
        t.insert(TrackName::Microphone, "hello world".to_string());
        assert_eq!(build_transcript(&t), "hello world");
    }

    #[test]
    fn build_transcript_only_empty_track_returns_empty_string() {
        let mut t = HashMap::new();
        t.insert(TrackName::Microphone, String::new());
        assert_eq!(build_transcript(&t), "");
    }

    #[test]
    fn build_transcript_drops_empty_tracks_when_aggregating() {
        // Microphone empty, system non-empty -> single-track plain
        // text, no header.
        let mut t = HashMap::new();
        t.insert(TrackName::Microphone, String::new());
        t.insert(TrackName::System, "system text".to_string());
        assert_eq!(build_transcript(&t), "system text");
    }

    #[test]
    fn build_transcript_two_tracks_renders_markdown_headers_in_fixed_order() {
        let mut t = HashMap::new();
        // Insert system first to make sure the order in the output is
        // driven by the [Microphone, System] ordering, not insertion
        // order.
        t.insert(TrackName::System, "system text".to_string());
        t.insert(TrackName::Microphone, "mic text".to_string());

        let out = build_transcript(&t);
        let expected = format!(
            "## {}\n\nmic text\n\n## {}\n\nsystem text",
            TrackName::Microphone.display_label(),
            TrackName::System.display_label(),
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn build_transcript_two_tracks_one_empty_does_not_render_headers() {
        // Per the existing logic, a single non-empty track skips the
        // headers entirely — even when a sibling track was present in
        // the map but came back blank.
        let mut t = HashMap::new();
        t.insert(TrackName::System, "".to_string());
        t.insert(TrackName::Microphone, "mic only".to_string());
        assert_eq!(build_transcript(&t), "mic only");
    }
}
