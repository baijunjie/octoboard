//! Opening an agent's desktop application at a URL its scheme names, on the machine the daemon
//! runs on. The host's own opener hands the URL to whichever application registered the scheme
//! and returns once it has accepted it, so it is waited on like the other launchers: Windows
//! through the shell's own call, as for any path, the other hosts through a command.

use std::path::Path;

use crate::platform::open::native_app::url_launch_for;
use crate::platform::open::paths::shell_open;

/// Opens `url` with the application registered for its scheme. `Err` is the reason it could not be.
pub fn open_url(url: &str) -> Result<(), String> {
    if let Some(opened) = shell_open(Path::new(url)) {
        // The shell words its failures for a file or a folder, which a link is neither.
        return opened.map_err(|_| "no application opened the link".to_string());
    }
    let command = url_launch_for(std::env::consts::OS, url)?;
    super::start(false, command)
}
