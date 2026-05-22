//! Global (system-wide) keyboard shortcut registration. Today there
//! is one binding — `Cmd+Shift+R` toggles recording — but isolating
//! it from `lib.rs` keeps the Tauri builder setup readable and gives
//! future shortcuts a single home to land in.
//!
//! Per ADR-0001 this is a smoke-only module. The
//! `tauri-plugin-global-shortcut` binding talks to native macOS key
//! event APIs; unit-testing it requires a running AppHandle and the
//! actual key event. Coverage is the manual smoke flow: press
//! Cmd+Shift+R, verify the recording starts/stops and the tray
//! reflects the new status.

use tauri::{AppHandle, Wry};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

/// Register the app's global keyboard shortcuts against the supplied
/// AppHandle. Call from inside Tauri's `setup` hook, after the
/// `tauri_plugin_global_shortcut` plugin has been installed.
pub fn register(app: &AppHandle<Wry>) -> anyhow::Result<()> {
    let toggle_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyR);
    let app_handle = app.clone();

    app.global_shortcut().on_shortcut(
        toggle_shortcut,
        move |_app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let h = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    crate::commands::toggle_recording_impl(&h).await;
                });
            }
        },
    )?;

    Ok(())
}
