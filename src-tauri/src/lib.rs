mod audio;
mod boot;
mod commands;
mod config;
mod icons;
mod model;
mod pipeline;
mod session;
mod shortcuts;
mod state;
mod transcription;
mod tray;
mod windows;

use state::AppState;
use std::sync::atomic::Ordering;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // The recording popover is a fixed-size pill,
                // always re-centered on show. Persisting bounds
                // would let an old 480x320 state override the new
                // compact size on next launch.
                .with_denylist(&[windows::RECORDING])
                // Persist size + position only; we do NOT want the
                // plugin to restore window visibility, otherwise
                // closing Settings between sessions would re-open
                // it on next launch (or on the activation flip
                // that fires when stopping a recording).
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION,
                )
                .build(),
        )
        .manage(AppState::new(config::load_config()))
        .on_menu_event(tray::handle_menu_event)
        .invoke_handler(tauri::generate_handler![
            commands::model::get_model_status,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::transcriptions::get_transcriptions,
            commands::transcriptions::get_transcription_body,
            commands::transcriptions::delete_transcription,
            commands::settings::set_output_folder,
            commands::settings::list_audio_devices,
            commands::settings::set_mic_device,
            commands::settings::set_sys_device,
            commands::recording::get_audio_levels,
            commands::settings::set_mic_enabled,
            commands::settings::set_sys_enabled,
            commands::session::check_incomplete_sessions,
            commands::session::recover_session,
            commands::session::dismiss_session,
            commands::get_app_status,
            commands::settings::get_settings,
            commands::windows::show_main_window,
            commands::windows::hide_main_window,
            commands::windows::show_settings_window,
        ])
        .setup(|app| {
            // Create the three top-level windows hidden so the boot
            // task can populate AppState before any UI surfaces a
            // "model not ready" splash to the user.
            windows::create_all(app.handle())?;
            windows::intercept_close_as_hide(app.handle());
            windows::init_activation_policy(app.handle());

            tray::setup_tray(app.handle())?;

            // Replace the bundled placeholder icon with our rasterised
            // dock badge so Cmd+Tab / the macOS Dock carries the same
            // family identity as the rest of the app while running.
            // No-op on non-macOS.
            icons::apply_dock_icon();

            // Register global keyboard shortcuts:
            //   Cmd+Shift+R  toggle recording
            //   Cmd+Shift+H  toggle Main window
            shortcuts::register(app.handle())?;

            // Spawn per-Track capture threads + the 5-minute
            // chunk-interval task.
            pipeline::start(app.handle());

            // Run the Whisper model boot sequence in the background.
            // Windows show a splash while it completes; recording is
            // blocked until `ModelStatus::Ready`.
            boot::spawn(app.handle());

            // Periodic tray title update (every second) for live timer.
            // Only updates title/tooltip — does NOT rebuild the menu to
            // avoid use-after-free when the menu is open on macOS.
            let tray_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let s = tray_handle.state::<AppState>();
                    let status = s.app_status.load(Ordering::Relaxed);
                    if status != state::STATUS_IDLE {
                        tray::update_tray_title(&tray_handle);
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
