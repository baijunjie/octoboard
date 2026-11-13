//! What stands for systemd user services on a Unix host that has none: no daemon is the main
//! process of one, and nothing is started as one. The same surface as `systemd.rs`, so that
//! `unix.rs` is the same on every Unix host.

use std::io;
use std::process::Command;

/// No systemd user manager runs a daemon here.
pub(crate) fn user_service_unit(_managed: bool) -> Option<String> {
    None
}

/// No systemd service runs the daemon here.
pub(crate) fn in_systemd_service() -> bool {
    false
}

/// Never called here, where [`user_service_unit`] names no service.
pub(crate) fn start_in_user_unit(
    _command: &Command,
    _unit: &str,
    _description: &str,
    _daemon: &str,
) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}
