//! Opening the system terminal in a project's directory, on the machine the daemon runs on.
//!
//! The desktop window is only a client, and a browser client may be another machine, so the
//! terminal is started here rather than in the window. On macOS that program is Terminal.app:
//! `open -a Terminal` hands it the directory and returns once Terminal has accepted it, which
//! starts a shell there. On Linux there is no single system terminal, so the first program in a
//! fixed list that can be given a directory is used, and on Windows Windows Terminal or else a
//! console window of `cmd`. A terminal emulator stays running for the
//! life of its window, so it is started in its own session and reaped in the background: waiting
//! for it would hold the request open until the user closed the window, and leaving it in the
//! daemon's process group would close it when the daemon exits.

use std::path::Path;
use std::process::Command;

use crate::platform::open::terminal::{host_launch, Launch};
use crate::protocol::{error_code, CodedError};

/// Opens the system terminal at `directory`. `directory` is a project's stored path.
pub fn open_directory(directory: &Path) -> Result<(), anyhow::Error> {
    crate::hostfs::require_directory(directory).map_err(|err| unavailable(&err.to_string()))?;
    // Where a terminal is looked for (Linux's list, Windows Terminal) it is looked for on one
    // login-shell snapshot. A snapshot that did not run is that failure: treating it as "not
    // installed" would report a missing terminal when the lookup never happened.
    let launch = host_launch(
        std::env::consts::OS,
        directory,
        || crate::env_shell::cached_snapshot().map_err(|err| format!("{err:#}")),
        |name, env| crate::env_shell::resolve_program(name, env).ok(),
    )
    .map_err(|detail| unavailable(&detail))?;
    start(&launch).map_err(|detail| unavailable(&detail))
}

fn unavailable(detail: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::TERMINAL_UNAVAILABLE,
        format!("The system terminal could not be opened: {detail}"),
        &[("detail", detail)],
    )
}

fn start(launch: &Launch) -> Result<(), String> {
    let mut command = Command::new(&launch.program);
    command.args(&launch.args);
    if let Some(directory) = &launch.directory {
        command.current_dir(directory);
    }
    super::start(!launch.wait, command)
}
