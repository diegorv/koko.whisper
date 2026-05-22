//! Whisper model lifecycle Tauri commands. Thin glue over `model/`
//! and `transcription/` — checks whether the ggml file is on disk,
//! downloads it with progress events, and instantiates a
//! `WhisperContext` into `AppState`.
//!
//! Per ADR-0001 these are smoke-only — each command is a few lines of
//! delegation over modules whose pure cores are already tested in
//! `model::tests` and `transcription::tests`.

use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};

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
