//! Global (system-wide) keyboard shortcut registration. Two bindings
//! today:
//!
//!   Cmd+Shift+R  toggle recording
//!   Cmd+Shift+H  toggle the Main (history) window
//!
//! Per ADR-0001 this is a smoke-only module. The
//! `tauri-plugin-global-shortcut` binding talks to native macOS key
//! event APIs; unit-testing it requires a running AppHandle and the
//! actual key event. Coverage is the manual smoke flow.

use tauri::{AppHandle, Wry};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

pub fn register(app: &AppHandle<Wry>) -> anyhow::Result<()> {
    let recording_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyR);
    let history_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyH);

    let recording_app = app.clone();
    app.global_shortcut().on_shortcut(
        recording_shortcut,
        move |_app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let h = recording_app.clone();
            tauri::async_runtime::spawn(async move {
                crate::commands::toggle_recording_impl(&h).await;
            });
        },
    )?;

    let history_app = app.clone();
    app.global_shortcut().on_shortcut(
        history_shortcut,
        move |_app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            crate::windows::toggle_main(&history_app);
        },
    )?;

    Ok(())
}
