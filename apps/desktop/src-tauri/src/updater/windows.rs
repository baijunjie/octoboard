//! The updater on every target but macOS, Windows above all (Linux builds it, and has no updater
//! of its own to ship).
//!
//! The plugin's install on Windows starts the NSIS installer and ends the process at once
//! (`std::process::exit`), skipping the quit confirmation, the daemon's stop and the exit
//! event's work. So `install` only downloads, and the installer starts from the restart
//! (`install_for_restart`) or on the way out (`install_on_quit`), after the exit flow has done its
//! own.

use tauri::AppHandle;
use tauri_plugin_updater::Update;

use crate::diagnostics::diagnostic;
use crate::tray;
use crate::window_state;

/// What goes between a channel's name and `.json` in the Windows manifests, which are kept apart
/// from the macOS ones (`stable-windows.json`, `beta-windows.json`).
pub const MANIFEST_SUFFIX: &str = "-windows";

/// Downloads `update` and hands the installer's bytes back for `install_for_restart` to run.
pub async fn install(
    update: &Update,
    on_chunk: impl FnMut(usize, Option<u64>),
) -> tauri_plugin_updater::Result<Option<Vec<u8>>> {
    update.download(on_chunk, || {}).await.map(Some)
}

/// Starts the installer when one was downloaded, which ends the process and, in passive mode,
/// relaunches the application once it is done. Returns without one, or when it cannot be started,
/// for the caller to relaunch the current version.
///
/// The process ends inside the installer's launch without a `RunEvent::Exit`, so what that event
/// saves is saved here first, and the tray icon, which the exit would remove, is removed. The daemon
/// has been stopped by then (`confirm_quit`).
pub fn install_for_restart(app: &AppHandle, held: Option<(Update, Vec<u8>)>) {
    if let Some((update, bytes)) = held {
        start_installer(app, update, bytes);
        diagnostic!("restarting without the update");
    }
}

/// Starts the installer when one was downloaded, without the relaunch, so the update is in place
/// for the next launch as on macOS; the installer's passive window shows its progress. Returns
/// without one, or when it cannot be started, for the caller to end the application. Saves and
/// removes what `install_for_restart` does, for the same reason.
pub fn install_on_quit(app: &AppHandle, held: Option<(Update, Vec<u8>)>) {
    if let Some((update, bytes)) = held {
        start_installer(app, update.restart_after_install(false), bytes);
        diagnostic!("quitting without the update");
    }
}

/// Runs the installer, which ends the process; returns only when it could not be started.
fn start_installer(app: &AppHandle, update: Update, bytes: Vec<u8>) {
    window_state::save(app);
    tray::remove(app);
    if let Err(err) = update.install(bytes) {
        diagnostic!("cannot start the update installer: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_manifests_have_names_of_their_own() {
        assert_eq!(MANIFEST_SUFFIX, "-windows");
    }
}
