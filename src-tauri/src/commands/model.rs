//! Whisper model lifecycle Tauri commands. The boot sequence
//! (`check → download → init`) runs in Rust `setup()` (see
//! `crate::boot`); the frontend just reads the current snapshot via
//! `get_model_status` and subscribes to the `model-status` event for
//! transitions.
//!
//! Per ADR-0001 these are smoke-only — the pure cores live under
//! `model::tests` and `transcription::tests`.

use crate::state::{AppState, ModelStatus};
use tauri::State;

/// Returns the current snapshot of the Whisper model boot lifecycle.
/// Each window calls this from its `onMount` before subscribing to
/// the `model-status` event so a window opened after the boot task
/// completed still sees `Ready` (events fired before the listener
/// was attached are otherwise lost).
#[tauri::command]
pub async fn get_model_status(state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let guard = state.model_status.lock().await;
    Ok(guard.clone())
}
