use crate::state::{AppState, TrackName, STATUS_RECORDING, STATUS_TRANSCRIBING};
use std::sync::atomic::Ordering;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    AppHandle, Manager, Wry,
};

/// Sets up the initial tray menu from current state.
/// Event handling is done at the app level via `handle_menu_event`.
pub fn setup_tray(app: &AppHandle) -> anyhow::Result<()> {
    if let Some(tray) = app.tray_by_id("main_tray") {
        let info = read_state(app);
        let menu = build_menu(app, &info)?;
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

struct TrayInfo {
    status: u8,
    mic_enabled: bool,
    sys_enabled: bool,
    elapsed: Option<std::time::Duration>,
}

fn read_state(app: &AppHandle) -> TrayInfo {
    let state = app.state::<AppState>();
    let status = state.app_status.load(Ordering::Relaxed);
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
    let elapsed = if status == STATUS_RECORDING {
        state
            .recording_started_at
            .lock()
            .ok()
            .and_then(|guard| guard.map(|t| t.elapsed()))
    } else {
        None
    };
    TrayInfo {
        status,
        mic_enabled,
        sys_enabled,
        elapsed,
    }
}

fn format_elapsed(d: std::time::Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

/// Builds the tray menu with recording controls and track toggles.
fn build_menu(app: &AppHandle, info: &TrayInfo) -> anyhow::Result<Menu<Wry>> {
    let (status_text, toggle_label, toggle_enabled) = match info.status {
        STATUS_RECORDING => {
            let timer = info
                .elapsed
                .map(format_elapsed)
                .unwrap_or_else(|| "00:00".to_string());
            (
                format!("● Gravando...  {}", timer),
                "Parar Gravacao (Cmd+Shift+R)".to_string(),
                true,
            )
        }
        STATUS_TRANSCRIBING => (
            "Transcrevendo...".to_string(),
            "Aguarde...".to_string(),
            false,
        ),
        _ => (
            "Parado".to_string(),
            "Iniciar Gravacao (Cmd+Shift+R)".to_string(),
            true,
        ),
    };

    let status = MenuItem::with_id(app, "status", &status_text, false, None::<&str>)?;
    let toggle = MenuItem::with_id(
        app,
        "toggle_recording",
        &toggle_label,
        toggle_enabled,
        None::<&str>,
    )?;
    let mic = CheckMenuItem::with_id(
        app,
        "toggle_mic",
        "Microfone",
        true,
        info.mic_enabled,
        None::<&str>,
    )?;
    let sys = CheckMenuItem::with_id(
        app,
        "toggle_sys",
        "Audio do Sistema",
        true,
        info.sys_enabled,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(app, "show_window", "Ver Transcricoes", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &status, &sep1, &toggle, &sep2, &mic, &sys, &sep3, &show, &quit,
        ],
    )?;

    Ok(menu)
}

/// Handles tray menu item clicks. Registered at the app level via `.on_menu_event()`.
pub fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "toggle_recording" => {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::commands::toggle_recording_impl(&app_clone).await;
            });
        }
        "toggle_mic" => {
            toggle_track_enabled(app, TrackName::Microphone);
            update_tray_menu(app);
        }
        "toggle_sys" => {
            toggle_track_enabled(app, TrackName::System);
            update_tray_menu(app);
        }
        "show_window" => {
            show_panel(app);
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

/// Toggles a track's enabled state and signals the capture thread.
fn toggle_track_enabled(app: &AppHandle, track_name: TrackName) {
    let state = app.state::<AppState>();
    if let Some(track) = state.tracks.get(&track_name) {
        let new_val = !track.enabled.load(Ordering::Relaxed);
        track.enabled.store(new_val, Ordering::Relaxed);

        let device_mutex = track.device.clone();
        let change_tx = track.change_tx.clone();
        let is_sys = track_name == TrackName::System;

        tauri::async_runtime::spawn(async move {
            if is_sys && new_val {
                // Auto-select first system device when enabling with none selected
                let has_device = device_mutex.lock().await.is_some();
                if !has_device {
                    if let Ok(devices) = crate::audio::list_input_devices() {
                        if let Some(first_sys) = devices.iter().find(|d| {
                            matches!(d.device_type, crate::audio::DeviceType::System)
                        }) {
                            let selected = crate::audio::SelectedDevice {
                                name: first_sys.name.clone(),
                                device_type: crate::audio::DeviceType::System,
                            };
                            *device_mutex.lock().await = Some(selected.clone());
                            let _ = change_tx.send(Some(selected));
                            return;
                        }
                    }
                }
            }
            let d = device_mutex.lock().await.clone();
            let _ = change_tx.send(d);
        });
    }
}

/// Rebuilds the tray menu, title, and tooltip from current AppState.
/// Call this on actual state changes (start/stop recording, toggle tracks).
pub fn update_tray_menu(app: &AppHandle) {
    let info = read_state(app);

    if let Some(tray) = app.tray_by_id("main_tray") {
        if let Ok(menu) = build_menu(app, &info) {
            let _ = tray.set_menu(Some(menu));
        }
        apply_tray_decorations(&tray, &info);
    }
}

/// Updates only the tray title and tooltip (safe to call frequently).
/// Does NOT rebuild the menu — avoids use-after-free when menu is open.
pub fn update_tray_title(app: &AppHandle) {
    let info = read_state(app);
    if let Some(tray) = app.tray_by_id("main_tray") {
        apply_tray_decorations(&tray, &info);
    }
}

fn apply_tray_decorations(tray: &tauri::tray::TrayIcon, info: &TrayInfo) {
    let title = match info.status {
        STATUS_RECORDING => {
            let timer = info
                .elapsed
                .map(format_elapsed)
                .unwrap_or_else(|| "00:00".to_string());
            Some(timer)
        }
        STATUS_TRANSCRIBING => Some("...".to_string()),
        _ => None,
    };
    let _ = tray.set_title(title.as_deref());

    let tooltip = match info.status {
        STATUS_RECORDING => "Koko Notes Whisper - Gravando...",
        STATUS_TRANSCRIBING => "Koko Notes Whisper - Transcrevendo...",
        _ => "Koko Notes Whisper",
    };
    let _ = tray.set_tooltip(Some(tooltip));
}

fn show_panel(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let _ = tauri::WebviewWindowBuilder::new(
            app,
            "main",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("Koko Notes Whisper")
        .inner_size(400.0, 520.0)
        .resizable(false)
        .build();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_format_elapsed_zero() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "00:00");
    }

    #[test]
    fn test_format_elapsed_seconds() {
        assert_eq!(format_elapsed(Duration::from_secs(5)), "00:05");
        assert_eq!(format_elapsed(Duration::from_secs(59)), "00:59");
    }

    #[test]
    fn test_format_elapsed_minutes() {
        assert_eq!(format_elapsed(Duration::from_secs(60)), "01:00");
        assert_eq!(format_elapsed(Duration::from_secs(65)), "01:05");
        assert_eq!(format_elapsed(Duration::from_secs(600)), "10:00");
        assert_eq!(format_elapsed(Duration::from_secs(3599)), "59:59");
    }

    #[test]
    fn test_format_elapsed_hours() {
        assert_eq!(format_elapsed(Duration::from_secs(3600)), "01:00:00");
        assert_eq!(format_elapsed(Duration::from_secs(3661)), "01:01:01");
        assert_eq!(format_elapsed(Duration::from_secs(7200)), "02:00:00");
        assert_eq!(format_elapsed(Duration::from_secs(36000)), "10:00:00");
    }
}
