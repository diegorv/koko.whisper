//! Window lifecycle management. Owns the three top-level windows
//! (`main`, `recording`, `settings`) and the macOS activation policy
//! flip that ties `main`'s visibility to whether the app participates
//! in the Dock and Cmd+Tab.
//!
//! The app boots as `Accessory` (tray-only, no Dock icon). Showing
//! `main` flips activation to `Regular`; hiding `main` flips it back.
//! `recording` and `settings` do not flip activation — they ride along
//! whichever policy `main` set, so summoning a recording popover from
//! the tray never hijacks macOS focus.

use tauri::{
    AppHandle, Manager, PhysicalPosition, Position, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const MAIN: &str = "main";
pub const RECORDING: &str = "recording";
pub const SETTINGS: &str = "settings";

const MAIN_W: f64 = 900.0;
const MAIN_H: f64 = 600.0;
const MAIN_MIN_W: f64 = 700.0;
const MAIN_MIN_H: f64 = 450.0;

const RECORDING_W: f64 = 240.0;
const RECORDING_H: f64 = 52.0;

const SETTINGS_W: f64 = 840.0;
const SETTINGS_H: f64 = 600.0;
const SETTINGS_MIN_W: f64 = 640.0;
const SETTINGS_MIN_H: f64 = 400.0;

/// Create all three windows hidden at app boot. Each routes to its
/// SvelteKit URL; bounds are persisted by `tauri-plugin-window-state`,
/// which restores size + position the next time the window is shown.
pub fn create_all(app: &AppHandle) -> tauri::Result<()> {
    let _main = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("/".into()))
        .title("Koko Notes Whisper")
        .inner_size(MAIN_W, MAIN_H)
        .min_inner_size(MAIN_MIN_W, MAIN_MIN_H)
        .resizable(true)
        .visible(false)
        .center()
        .build()?;

    let _recording = WebviewWindowBuilder::new(app, RECORDING, WebviewUrl::App("/recording".into()))
        .title("")
        .inner_size(RECORDING_W, RECORDING_H)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        // Pill renders its own drop shadow via box-shadow on the
        // visible element; the OS window shadow would otherwise
        // outline the full transparent rect, not the pill.
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;

    let _settings = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("/settings".into()))
        .title("Settings")
        .inner_size(SETTINGS_W, SETTINGS_H)
        .min_inner_size(SETTINGS_MIN_W, SETTINGS_MIN_H)
        .resizable(true)
        .visible(false)
        .center()
        .build()?;

    Ok(())
}

fn window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    app.get_webview_window(label)
}

#[cfg(target_os = "macos")]
fn set_activation_policy(app: &AppHandle, regular: bool) {
    let policy = if regular {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };
    let _ = app.set_activation_policy(policy);
}

#[cfg(not(target_os = "macos"))]
fn set_activation_policy(_app: &AppHandle, _regular: bool) {}

/// Apply the initial Accessory policy at boot. Called once from
/// `setup()`. macOS-only; a no-op on other platforms.
pub fn init_activation_policy(app: &AppHandle) {
    set_activation_policy(app, false);
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = window(app, MAIN) {
        set_activation_policy(app, true);
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn hide_main(app: &AppHandle) {
    if let Some(w) = window(app, MAIN) {
        let _ = w.hide();
        set_activation_policy(app, false);
    }
}

pub fn toggle_main(app: &AppHandle) {
    let visible = window(app, MAIN)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_main(app);
    } else {
        show_main(app);
    }
}

/// Anchor the pill to the top-right of the active monitor, just
/// below the macOS menu bar where the tray icon lives. Called from
/// `show_recording` on every show so the popover never drifts away
/// from the tray icon between sessions.
fn position_recording_top_right(w: &WebviewWindow) {
    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let monitor_size = m.size();
    let monitor_pos = m.position();
    let Ok(win_size) = w.outer_size() else { return };
    let scale = m.scale_factor();
    // Offsets in logical pixels — convert to physical to match the
    // monitor / window units (cpal returns physical sizes).
    let right_margin = (12.0 * scale) as i32;
    let top_margin = (32.0 * scale) as i32;
    let x = monitor_pos.x + (monitor_size.width as i32) - (win_size.width as i32) - right_margin;
    let y = monitor_pos.y + top_margin;
    let _ = w.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

pub fn show_recording(app: &AppHandle) {
    if let Some(w) = window(app, RECORDING) {
        position_recording_top_right(&w);
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn hide_recording(app: &AppHandle) {
    if let Some(w) = window(app, RECORDING) {
        let _ = w.hide();
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(w) = window(app, SETTINGS) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Intercept the close button on all three windows so closing hides
/// instead of destroying. Re-opening a window via the tray or a
/// shortcut then re-uses the existing instance (state survives).
pub fn intercept_close_as_hide(app: &AppHandle) {
    for label in [MAIN, RECORDING, SETTINGS] {
        if let Some(w) = window(app, label) {
            let app_clone = app.clone();
            let label_str = label.to_string();
            w.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if let Some(win) = window(&app_clone, &label_str) {
                        let _ = win.hide();
                    }
                    if label_str == MAIN {
                        set_activation_policy(&app_clone, false);
                    }
                    api.prevent_close();
                }
            });
        }
    }
}
