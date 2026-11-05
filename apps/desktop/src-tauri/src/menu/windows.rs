//! The menu bar on Windows: none. Tauri would attach an app menu to the window as a menu bar. This
//! module serves every target but macOS, Linux included.

use std::collections::HashMap;

use tauri::AppHandle;

/// Sets no menu.
pub fn install(_app: &AppHandle) -> tauri::Result<()> {
    Ok(())
}

/// Accepts the UI's labels and keeps none, as there is no menu to put them in.
pub fn set_labels(
    _app: &AppHandle,
    _labels: HashMap<String, String>,
    _settings_accelerator: Option<String>,
) -> Result<(), String> {
    Ok(())
}
