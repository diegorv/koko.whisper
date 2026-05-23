//! Model boot orchestration. Runs once at app startup from `setup()`
//! and drives the Whisper model through its lifecycle so windows can
//! show a splash while it loads instead of each one re-running the
//! sequence in its own `onMount` (the ADR-0001 layout).
//!
//! Lifecycle:
//!   Unchecked → Downloading{progress} → Ready
//!                                       Error{message}
//!
//! Each transition writes the new state to `AppState.model_status`
//! AND emits a `model-status` event so windows that were already open
//! see the change without polling. `Downloading` also emits the
//! existing `model-download-progress` event for the per-chunk
//! progress bar.

use crate::state::{AppState, ModelStatus};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

/// Spawn the boot task. Returns immediately; the task runs to
/// completion in the background and the model status mutates as it
/// progresses.
pub fn spawn(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        run(&handle).await;
    });
}

async fn run(app: &AppHandle) {
    let state = app.state::<AppState>();
    let status = state.model_status.clone();

    if !crate::model::is_model_downloaded() {
        set_status(app, &status, ModelStatus::Downloading { progress: 0.0 }).await;

        let progress_app = app.clone();
        let progress_status = status.clone();
        let download_result = crate::model::download_model(move |progress| {
            let _ = progress_app.emit("model-download-progress", progress);
            // Update the stored snapshot too so a window opened
            // mid-download reads the right value via get_model_status.
            let progress_app_inner = progress_app.clone();
            let progress_status_inner = progress_status.clone();
            tauri::async_runtime::spawn(async move {
                let mut guard = progress_status_inner.lock().await;
                if let ModelStatus::Downloading { progress: ref mut p } = *guard {
                    *p = progress;
                }
                drop(guard);
                let _ = progress_app_inner
                    .emit("model-status", ModelStatus::Downloading { progress });
            });
        })
        .await;

        if let Err(e) = download_result {
            set_status(
                app,
                &status,
                ModelStatus::Error {
                    message: format!("Failed to download model: {e}"),
                },
            )
            .await;
            return;
        }
    }

    let init_result: Result<(), String> = (|| async {
        let model_path = crate::model::get_model_path().map_err(|e| e.to_string())?;
        let ctx = crate::transcription::create_whisper_context(&model_path)
            .map_err(|e| e.to_string())?;
        let mut guard = state.whisper_context.lock().await;
        *guard = Some(ctx);
        Ok(())
    })()
    .await;

    match init_result {
        Ok(_) => set_status(app, &status, ModelStatus::Ready).await,
        Err(e) => {
            set_status(
                app,
                &status,
                ModelStatus::Error {
                    message: format!("Failed to load model: {e}"),
                },
            )
            .await
        }
    }
}

async fn set_status(app: &AppHandle, slot: &Arc<Mutex<ModelStatus>>, next: ModelStatus) {
    let mut guard = slot.lock().await;
    *guard = next.clone();
    drop(guard);
    let _ = app.emit("model-status", next);
}
