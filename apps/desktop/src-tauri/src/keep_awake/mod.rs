//! Keeping the computer and its display awake, which is the UI's Settings choice (Prevent sleep).
//! The UI decides when to hold (always, or while a session is working) and hands the shell only
//! the outcome through `set_keep_awake`: the shell holds a power assertion while told to and
//! releases it when told not to. Nothing is held at launch, so a window that never gets to say so
//! (the daemon failed to start) leaves the system to sleep as usual. The system releases a hold
//! along with the process, so a quit or a crash needs no cleanup of ours. While the hold is up the
//! menu bar icon shows it (`tray::keep_awake_changed`).
//!
//! What the hold takes on each platform is in `macos` and `windows`; the latter serves every target
//! but macOS, Linux included, where there is nothing to hold.

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

/// The hold while there is one; dropping it releases it.
#[derive(Default)]
pub struct KeepAwakeState {
    hold: Mutex<Option<platform::Hold>>,
}

/// Starts or ends the hold on the system's idle sleep and idle display sleep. Holding while
/// already holding, and releasing while not holding, change nothing.
#[tauri::command]
pub fn set_keep_awake(app: AppHandle, held: bool) -> Result<(), String> {
    let state = app.state::<KeepAwakeState>();
    let mut hold = state
        .hold
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !held {
        *hold = None;
    } else if hold.is_none() {
        *hold = Some(platform::Hold::acquire()?);
    }
    let held = hold.is_some();
    drop(hold);
    // Best-effort: an icon left on its previous look is not worth failing the hold for.
    let _ = crate::tray::keep_awake_changed(&app, held);
    Ok(())
}
