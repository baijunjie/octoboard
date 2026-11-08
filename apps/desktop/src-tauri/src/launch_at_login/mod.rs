//! Opening the application when the user logs in, which the UI's Settings switch (Launch at login)
//! offers. The switch shows the system's own state, read when the section opens, and changes it
//! through `set_launch_at_login`: the user can also turn the entry off in the system's own list
//! (System Settings' Login Items on macOS, Task Manager's Startup apps on Windows), so nothing here
//! is remembered.
//!
//! A launch the system made for the entry is told apart from any other (`capture_launch`), which is
//! what lets the window stay hidden after it when the user keeps the app running in the background
//! (`background::reveal_main_window`): the choice to run in the background is the UI's, handed over
//! when it reveals the window.
//!
//! What the entry is on each platform is in `macos` and `windows`; the latter serves every target
//! but macOS, Linux included, where there is no entry to make.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod windows;
#[cfg(not(target_os = "macos"))]
use windows as platform;

/// The command-line argument of the Windows entry, which is how a launch by it is recognised.
#[cfg_attr(target_os = "macos", allow(dead_code))]
const LOGIN_ARGUMENT: &str = "--launched-at-login";

/// Where the entry stands, as the UI words it.
#[derive(Serialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Enabled,
    Disabled,
    /// Registered, but the system waits for the user to allow it (macOS, System Settings' Login
    /// Items).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    RequiresApproval,
}

/// Whether this process was started by the system for the login entry, until the window is first
/// revealed (`take_login_launch`).
static LOGIN_LAUNCH: AtomicBool = AtomicBool::new(false);

/// Whether this process was started by the system for the login entry, once: answering clears it,
/// as only the first reveal of a process can be that launch (`background::reveal_main_window`).
pub fn take_login_launch() -> bool {
    LOGIN_LAUNCH.swap(false, Ordering::SeqCst)
}

/// Whether the system started this process for the login entry and the window has not been revealed
/// yet, which leaves the reveal to decide whether the window shows at all.
pub fn login_launch_pending() -> bool {
    LOGIN_LAUNCH.load(Ordering::SeqCst)
}

/// Whether this machine can open the app at login; what the host facts announce.
pub fn supported() -> bool {
    platform::supported()
}

/// Records whether the system started this process for the login entry. Called once, from `setup`
/// on the main thread, where the macOS launch event is still the current one.
pub fn capture_launch() {
    LOGIN_LAUNCH.store(platform::launched_at_login(), Ordering::SeqCst);
}

/// Whether `args` carry the argument the Windows entry starts the app with.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn has_login_argument<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == std::ffi::OsStr::new(LOGIN_ARGUMENT))
}

/// Whether the system opens the app at login now.
#[tauri::command]
pub fn launch_at_login_status() -> Result<Status, String> {
    platform::status()
}

/// Registers or removes the login entry, and answers where it stands afterwards.
#[tauri::command]
pub fn set_launch_at_login(enabled: bool) -> Result<Status, String> {
    platform::set(enabled)
}

/// Opens System Settings at its list of login items, where an entry waiting for approval is
/// allowed.
#[tauri::command]
pub fn open_login_items_settings() -> Result<(), String> {
    platform::open_settings()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_argument_is_recognised_among_others() {
        assert!(has_login_argument(["octoboard.exe", LOGIN_ARGUMENT]));
        assert!(!has_login_argument(["octoboard.exe", "--other"]));
        assert!(!has_login_argument(Vec::<String>::new()));
    }

    #[test]
    fn a_status_is_worded_for_the_ui_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&Status::RequiresApproval).unwrap(),
            "\"requiresApproval\""
        );
    }
}
