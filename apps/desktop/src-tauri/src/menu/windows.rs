//! The menu bar on Windows: none. Tauri would attach an app menu to the window as a menu bar. What
//! stands for the menu's remote daemons here is every window's system menu (`system_menu`) and the
//! taskbar's jump list (`jump_list`), which the labels the UI sends name. This module serves every
//! target but macOS, Linux included, where there is neither.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::AppHandle;

/// The labels the UI last sent, in its language, which the jump list's and the system menus' lines
/// read: `menu.remote`, `menu.remoteDevices`, `menu.noRemoteDaemons` and `menu.openMainWindow`
/// (`useNativeMenuLabels`), in sentence case as Windows' own menus read, as are the English
/// defaults here.
static LABELS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Sets no menu, and the jump list and the system menus' items in English until the UI sends its
/// labels.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    rebuild_remote_lists(app);
    Ok(())
}

/// Gives a window just built its system menu's Remote items. On the main thread.
#[cfg(windows)]
pub fn install_window(window: &tauri::WebviewWindow) {
    super::system_menu::install(window);
}

/// No system menu to add to off Windows.
#[cfg(not(windows))]
pub fn install_window(_window: &tauri::WebviewWindow) {}

/// Rebuilds what lists the saved remote daemons, the jump list and every window's system menu, in
/// the language the UI last sent. On any thread.
pub fn remote_daemons_changed(app: &AppHandle) {
    rebuild_remote_lists(app);
}

/// Rebuilds the jump list and the system menus' items in the language the UI last sent. On any
/// thread.
fn rebuild_remote_lists(app: &AppHandle) {
    let labels = LABELS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default();
    let entries = crate::remote::menu_entries(app);
    #[cfg(windows)]
    super::system_menu::rebuild(app, &labels, &entries);
    let _ = app.run_on_main_thread(move || set_jump_list(&labels, &entries));
}

/// Keeps the UI's labels for the jump list and the system menus, the only things here they name,
/// and rebuilds them.
pub fn set_labels(
    app: &AppHandle,
    labels: HashMap<String, String>,
    _settings_accelerator: Option<String>,
) -> Result<(), String> {
    *LABELS.lock().unwrap_or_else(|e| e.into_inner()) = Some(labels);
    rebuild_remote_lists(app);
    Ok(())
}

/// The label under `key`, or its English default.
fn label<'a>(labels: &'a HashMap<String, String>, key: &str, default: &'a str) -> &'a str {
    labels.get(key).map_or(default, String::as_str)
}

#[cfg(windows)]
fn set_jump_list(labels: &HashMap<String, String>, entries: &[(String, String)]) {
    let (tasks, daemons) = super::jump_list::lines(
        label(labels, "menu.remoteDevices", "Remote devices"),
        label(labels, "menu.openMainWindow", "Open this PC’s window"),
        entries,
    );
    let category = label(labels, "menu.remote", "Remote");
    if let Err(err) = super::jump_list::set(category, &tasks, &daemons) {
        crate::diagnostics::diagnostic!("cannot set the taskbar's jump list: {err}");
    }
}

#[cfg(not(windows))]
fn set_jump_list(labels: &HashMap<String, String>, entries: &[(String, String)]) {
    let _ = (label(labels, "menu.remote", "Remote"), entries);
}
