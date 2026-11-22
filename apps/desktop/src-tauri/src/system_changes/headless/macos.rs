//! The macOS side of the clean-up before the uninstall: there is none to run. The application is
//! removed by moving it to the Trash, so nothing runs for its uninstall and no process is started
//! for a clean-up before it; the row in Settings is the clean-up.

pub fn has_uninstaller() -> bool {
    false
}

/// Never asked: no clean-up runs without a window here.
pub fn application_running(_identifier: &str) -> bool {
    false
}
