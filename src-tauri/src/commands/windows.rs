//! Window-control Tauri commands. Frontend invokes these from
//! keyboard shortcuts inside a focused window (e.g. `Cmd+,` from
//! `main` opening `settings`) and from buttons that link between
//! windows. The actual `show()` / `hide()` calls live in
//! `crate::windows` so the activation-policy flip stays in one place.

use tauri::AppHandle;

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    crate::windows::show_main(&app);
}

#[tauri::command]
pub fn hide_main_window(app: AppHandle) {
    crate::windows::hide_main(&app);
}

#[tauri::command]
pub fn show_settings_window(app: AppHandle) {
    crate::windows::show_settings(&app);
}
