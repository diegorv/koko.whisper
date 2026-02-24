use crate::audio::{self, DeviceType, SelectedDevice};
use crate::config::{self, AppConfig};
use crate::state::{
    ActiveSession, ActiveTrackSession, AppState, TrackName, STATUS_IDLE, STATUS_RECORDING,
    STATUS_TRANSCRIBING,
};
use chrono::Local;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

// --- Shared utilities ---

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

/// Auto-select the first available system audio device if none is configured.
/// Returns true if a device was auto-selected.
pub async fn auto_select_system_device(track: &crate::state::TrackState) -> bool {
    let has_device = track.device.lock().await.is_some();
    if has_device {
        return false;
    }
    if let Ok(devices) = audio::list_input_devices() {
        if let Some(first_sys) = devices
            .iter()
            .find(|d| matches!(d.device_type, DeviceType::System))
        {
            let selected = SelectedDevice {
                name: first_sys.name.clone(),
                device_type: DeviceType::System,
            };
            *track.device.lock().await = Some(selected.clone());
            let _ = track.change_tx.send(Some(selected));
            return true;
        }
    }
    false
}

/// Read current in-memory state and persist to config.json.
async fn save_current_config(state: &AppState) {
    let output_folder = state.output_folder.lock().await.to_string_lossy().to_string();

    let mic_device = match state.tracks.get(&TrackName::Microphone) {
        Some(t) => t.device.lock().await.clone(),
        None => None,
    };
    let sys_device = match state.tracks.get(&TrackName::System) {
        Some(t) => t.device.lock().await.clone(),
        None => None,
    };
    let mic_enabled = state
        .tracks
        .get(&TrackName::Microphone)
        .map(|t| t.enabled.load(Ordering::Relaxed))
        .unwrap_or(true);
    let sys_enabled = state
        .tracks
        .get(&TrackName::System)
        .map(|t| t.enabled.load(Ordering::Relaxed))
        .unwrap_or(false);

    let cfg = AppConfig {
        output_folder: Some(output_folder),
        mic_device,
        sys_device,
        mic_enabled,
        sys_enabled,
    };
    config::save_config(&cfg);
}

fn copy_to_clipboard(text: &str) {
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

fn save_markdown(output_folder: &PathBuf, transcript: &str) -> anyhow::Result<PathBuf> {
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

// --- Settings ---

#[derive(serde::Serialize, Clone)]
pub struct AppSettings {
    pub output_folder: String,
    pub mic_device: Option<SelectedDevice>,
    pub sys_device: Option<SelectedDevice>,
    pub mic_enabled: bool,
    pub sys_enabled: bool,
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let output_folder = state.output_folder.lock().await.to_string_lossy().to_string();
    let mic_device = match state.tracks.get(&TrackName::Microphone) {
        Some(t) => t.device.lock().await.clone(),
        None => None,
    };
    let sys_device = match state.tracks.get(&TrackName::System) {
        Some(t) => t.device.lock().await.clone(),
        None => None,
    };
    let mic_enabled = state
        .tracks
        .get(&TrackName::Microphone)
        .map(|t| t.enabled.load(Ordering::Relaxed))
        .unwrap_or(true);
    let sys_enabled = state
        .tracks
        .get(&TrackName::System)
        .map(|t| t.enabled.load(Ordering::Relaxed))
        .unwrap_or(false);

    Ok(AppSettings {
        output_folder,
        mic_device,
        sys_device,
        mic_enabled,
        sys_enabled,
    })
}

// --- Audio levels ---

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

// --- Track enable/disable ---

#[tauri::command]
pub async fn set_mic_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let track = state
        .tracks
        .get(&TrackName::Microphone)
        .ok_or("No microphone track")?;
    track.enabled.store(enabled, Ordering::Relaxed);
    let device = track.device.lock().await.clone();
    track.change_tx.send(device).map_err(|e| e.to_string())?;
    crate::tray::update_tray_menu(&app);
    save_current_config(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn set_sys_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<Option<SelectedDevice>, String> {
    let track = state
        .tracks
        .get(&TrackName::System)
        .ok_or("No system track")?;
    track.enabled.store(enabled, Ordering::Relaxed);

    if enabled {
        auto_select_system_device(track).await;
    }

    let device = track.device.lock().await.clone();
    track
        .change_tx
        .send(device.clone())
        .map_err(|e| e.to_string())?;
    crate::tray::update_tray_menu(&app);
    save_current_config(&state).await;
    Ok(device)
}

// --- Transcriptions ---

#[derive(serde::Serialize, Clone)]
pub struct TranscriptionEntry {
    pub filename: String,
    pub preview: String,
    pub path: String,
}

#[tauri::command]
pub async fn get_transcriptions(
    state: State<'_, AppState>,
) -> Result<Vec<TranscriptionEntry>, String> {
    let output_folder = state.output_folder.lock().await.clone();
    let mut entries = Vec::new();

    if output_folder.exists() {
        let mut paths: Vec<_> = std::fs::read_dir(&output_folder)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect();
        paths.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

        for entry in paths.iter().take(20) {
            let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
            // Extract text after the "---" separator and truncate for preview
            let preview = content
                .split("---")
                .nth(1)
                .unwrap_or(&content)
                .trim()
                .chars()
                .take(150)
                .collect::<String>();
            entries.push(TranscriptionEntry {
                filename: entry.file_name().to_string_lossy().to_string(),
                preview,
                path: entry.path().to_string_lossy().to_string(),
            });
        }
    }

    Ok(entries)
}

// --- Model ---

#[tauri::command]
pub async fn check_model_status() -> Result<bool, String> {
    Ok(crate::model::is_model_downloaded())
}

#[tauri::command]
pub async fn download_model(app: AppHandle) -> Result<(), String> {
    let app_clone = app.clone();
    crate::model::download_model(move |progress| {
        let _ = app_clone.emit("model-download-progress", progress);
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn initialize_whisper(state: State<'_, AppState>) -> Result<(), String> {
    let model_path = crate::model::get_model_path().map_err(|e| e.to_string())?;
    let ctx =
        crate::transcription::create_whisper_context(&model_path).map_err(|e| e.to_string())?;
    let mut guard = state.whisper_context.lock().await;
    *guard = Some(ctx);
    Ok(())
}

// --- Recording ---

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

// --- Output folder ---

#[tauri::command]
pub async fn set_output_folder(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let mut folder = state.output_folder.lock().await;
    *folder = PathBuf::from(path);
    drop(folder);
    save_current_config(&state).await;
    Ok(())
}

// --- Audio devices ---

#[tauri::command]
pub async fn list_audio_devices() -> Result<Vec<audio::AudioDevice>, String> {
    audio::list_input_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_mic_device(
    state: State<'_, AppState>,
    device: Option<SelectedDevice>,
) -> Result<(), String> {
    let track = state
        .tracks
        .get(&TrackName::Microphone)
        .ok_or("No microphone track")?;
    {
        let mut selected = track.device.lock().await;
        *selected = device.clone();
    }
    track.change_tx.send(device).map_err(|e| e.to_string())?;
    save_current_config(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn set_sys_device(
    state: State<'_, AppState>,
    device: Option<SelectedDevice>,
) -> Result<(), String> {
    let track = state
        .tracks
        .get(&TrackName::System)
        .ok_or("No system track")?;
    {
        let mut selected = track.device.lock().await;
        *selected = device.clone();
    }
    track.change_tx.send(device).map_err(|e| e.to_string())?;
    save_current_config(&state).await;
    Ok(())
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
    use super::*;

    #[test]
    fn test_build_transcript_single_track() {
        let mut transcripts = HashMap::new();
        transcripts.insert(TrackName::Microphone, "Hello world".to_string());
        assert_eq!(build_transcript(&transcripts), "Hello world");
    }

    #[test]
    fn test_build_transcript_single_system_track() {
        let mut transcripts = HashMap::new();
        transcripts.insert(TrackName::System, "System audio text".to_string());
        assert_eq!(build_transcript(&transcripts), "System audio text");
    }

    #[test]
    fn test_build_transcript_multi_track() {
        let mut transcripts = HashMap::new();
        transcripts.insert(TrackName::Microphone, "Minha fala".to_string());
        transcripts.insert(TrackName::System, "Fala do participante".to_string());
        let result = build_transcript(&transcripts);
        assert!(result.contains("## Eu (Microfone)"));
        assert!(result.contains("## Participante (Audio do Sistema)"));
        assert!(result.contains("Minha fala"));
        assert!(result.contains("Fala do participante"));
        // Microphone should come before System
        let mic_pos = result.find("Eu (Microfone)").unwrap();
        let sys_pos = result.find("Participante").unwrap();
        assert!(mic_pos < sys_pos);
    }

    #[test]
    fn test_build_transcript_one_empty_track() {
        let mut transcripts = HashMap::new();
        transcripts.insert(TrackName::Microphone, String::new());
        transcripts.insert(TrackName::System, "Only system".to_string());
        // One non-empty track → plain text (no headers)
        assert_eq!(build_transcript(&transcripts), "Only system");
    }

    #[test]
    fn test_build_transcript_all_empty() {
        let transcripts: HashMap<TrackName, String> = HashMap::new();
        assert_eq!(build_transcript(&transcripts), "");
    }

    #[test]
    fn test_build_transcript_both_empty_strings() {
        let mut transcripts = HashMap::new();
        transcripts.insert(TrackName::Microphone, String::new());
        transcripts.insert(TrackName::System, String::new());
        assert_eq!(build_transcript(&transcripts), "");
    }

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
