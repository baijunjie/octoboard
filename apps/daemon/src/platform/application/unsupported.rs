//! Hosts whose application the command line does not start or quit.

use std::path::Path;

use crate::platform::proc::{ProcessIdentity, Sent};

pub fn launch_in_background(_exe: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "the application cannot be started from the command line on this system",
    ))
}

pub fn of_daemon(_daemon: &ProcessIdentity) -> Option<ProcessIdentity> {
    None
}

pub fn runs_bundle_of(_app: &ProcessIdentity, _exe: &Path) -> bool {
    false
}

pub fn ask_to_quit(_app: &ProcessIdentity) -> Sent {
    Sent::Failed(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "the application cannot be asked to quit from the command line on this system",
    ))
}
