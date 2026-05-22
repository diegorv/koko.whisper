//! Settings Tauri commands and supporting helpers. Owns the
//! `/settings` page contract: output folder, per-Track enable/disable,
//! per-Track device selection, and the device-enumeration command.
//!
//! Per ADR-0001 these are smoke-only commands. They mutate AppState's
//! `Arc<Mutex<...>>` cells and dispatch on the watch channel that
//! triggers the cpal stream restart — exercised only by the real
//! `/settings` UI flow.

use crate::audio::{self, DeviceType, SelectedDevice};
use crate::config::{self, AppConfig};
use crate::state::{AppState, TrackName};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

#[derive(serde::Serialize, Clone)]
pub struct AppSettings {
    pub output_folder: String,
    pub mic_device: Option<SelectedDevice>,
    pub sys_device: Option<SelectedDevice>,
    pub mic_enabled: bool,
    pub sys_enabled: bool,
}

/// Auto-select the first available system audio device if none is
/// configured. Returns true if a device was auto-selected.
async fn auto_select_system_device(track: &crate::state::TrackState) -> bool {
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

/// Snapshot current in-memory state into `AppConfig` and persist it.
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
