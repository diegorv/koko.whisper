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

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const MAIN: &str = "main";
pub const RECORDING: &str = "recording";
pub const SETTINGS: &str = "settings";

const MAIN_W: f64 = 900.0;
const MAIN_H: f64 = 600.0;
const MAIN_MIN_W: f64 = 700.0;
const MAIN_MIN_H: f64 = 450.0;

const RECORDING_W: f64 = 480.0;
const RECORDING_H: f64 = 320.0;

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
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .center()
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

pub fn show_recording(app: &AppHandle) {
    if let Some(w) = window(app, RECORDING) {
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
