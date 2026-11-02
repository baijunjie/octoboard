//! Starting the host's own launchers: the system file manager and default applications
//! (`paths`) and the system terminal (`terminal`). A launcher is either waited on, for one that
//! exits once the host has accepted the request, or started in its own session and reaped in the
//! background, for one that lives as long as the application it opens. What each launcher is
//! asked is the caller's; this module owns the start, the bound on a waited one and the reading of
//! a failed launch.

use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::processes::subprocess::{run_bounded, Bounds, Reach};

pub mod paths;
pub mod terminal;

/// What a waited command may take and say (a launcher or the `mdls` type query): the host answers in
/// moments, so this only bounds a wedged one, not the application a launcher then opens.
const OPEN_BOUNDS: Bounds = Bounds {
    timeout: Duration::from_secs(15),
    stdout_limit: 16 * 1024,
    stderr_limit: 16 * 1024,
    reach: Reach::ProcessGroup,
    term_grace: Duration::ZERO,
};

/// Starts `command`. A detached one is handed to [`crate::processes::subprocess::spawn_detached`];
/// any other is run to its end within [`OPEN_BOUNDS`] and judged by its exit status.
fn start(detached: bool, mut command: Command) -> Result<(), String> {
    if detached {
        return crate::processes::subprocess::spawn_detached(command);
    }
    let output = run_bounded(&mut command, &OPEN_BOUNDS, &AtomicBool::new(false))
        .map_err(|err| err.to_string())?;
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
