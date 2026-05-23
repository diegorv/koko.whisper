use crate::state::AppState;
use std::sync::atomic::Ordering;
use tauri::State;

pub mod model;
pub mod recording;
pub mod session;
pub mod settings;
pub mod transcriptions;
pub mod windows;

// `toggle_recording_impl` is invoked from `tray::handle_menu_event` and
// `shortcuts::register` via `crate::commands::toggle_recording_impl(...)`,
// so the re-export must live here. The Tauri command functions are NOT
// re-exported because `tauri::generate_handler!` cannot follow re-exports
// — it needs both the fn and the `__cmd__<name>` macro shim at the same
// path. lib.rs references them via the full `commands::<submod>::*` path.
pub use recording::toggle_recording_impl;

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
