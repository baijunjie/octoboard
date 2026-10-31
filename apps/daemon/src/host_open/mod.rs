//! Starting the host's own launchers: the system file manager and default applications
//! (`paths`) and the system terminal (`terminal`). A launcher is either waited on, for one that
//! exits once the host has accepted the request, or started in its own session and reaped in the
//! background, for one that lives as long as the application it opens. What each launcher is
//! asked, and how long a waited one may take, is the caller's; this module owns the start and the
//! reading of a failed launch.

use std::process::{Command, Output};

pub mod paths;
pub mod terminal;

/// Starts `command`. A detached one is handed to [`crate::processes::subprocess::spawn_detached`];
/// any other is run by `run`, which returns its output or the detail of why it did not finish, and
/// is then judged by its exit status.
fn start(
    detached: bool,
    mut command: Command,
    run: impl FnOnce(&mut Command) -> Result<Output, String>,
) -> Result<(), String> {
    if detached {
        return crate::processes::subprocess::spawn_detached(command);
    }
    let output = run(&mut command)?;
    if output.status.success() {
        return Ok(());
    }
    for bytes in [&output.stderr, &output.stdout] {
        let message = String::from_utf8_lossy(bytes).trim().to_string();
        if !message.is_empty() {
            return Err(message);
        }
    }
    Err(format!("the launcher exited with {}", output.status))
}
