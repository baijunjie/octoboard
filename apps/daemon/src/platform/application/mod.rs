//! The desktop application as the command line of its own copy of `octoboardd` reaches it: where
//! the host lets the command line start it in the background and ask it to quit, so that `start`
//! and `stop` act on the application, whose daemon it is, and not on a daemon of their own. Only
//! the macOS application is reached this way; on every other host [`MANAGED`] is false and the
//! rest does nothing.

use std::path::Path;

use super::proc::{ProcessIdentity, Sent};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as host;

#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
use unsupported as host;

/// Whether the command line of the application's own copy starts and quits the application.
pub(crate) const MANAGED: bool = cfg!(target_os = "macos");

/// Starts the application whose copy of the daemon is `exe` in the background, with no window and
/// its menu bar icon alone, and returns once the host has accepted the launch.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn launch_in_background(exe: &Path) -> std::io::Result<()> {
    host::launch_in_background(exe)
}

/// The application that runs the daemon `daemon` names: its parent, which started it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn of_daemon(daemon: &ProcessIdentity) -> Option<ProcessIdentity> {
    host::of_daemon(daemon)
}

/// Whether the application `app` names runs from the application bundle that holds `exe`, the
/// copy of the daemon this process is: only an application of that bundle, a build with this
/// executable's handling of the quit request, is asked to quit.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn runs_bundle_of(app: &ProcessIdentity, exe: &Path) -> bool {
    host::runs_bundle_of(app, exe)
}

/// Asks the application `app` names to quit without asking the user.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn ask_to_quit(app: &ProcessIdentity) -> Sent {
    host::ask_to_quit(app)
}
