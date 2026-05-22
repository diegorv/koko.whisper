use crate::audio::{self, DeviceType, SelectedDevice};
use crate::config::{self, AppConfig};
use crate::state::{AppState, TrackName};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

pub mod recording;
pub mod transcriptions;

// `toggle_recording_impl` is invoked from `tray::handle_menu_event` and
// `shortcuts::register` via `crate::commands::toggle_recording_impl(...)`,
// so the re-export must live here. The Tauri command functions are NOT
// re-exported because `tauri::generate_handler!` cannot follow re-exports
// — it needs both the fn and the `__cmd__<name>` macro shim at the same
// path. lib.rs references them via the full `commands::<submod>::*` path.
pub use recording::toggle_recording_impl;

use recording::{build_transcript, copy_to_clipboard, save_markdown};

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
