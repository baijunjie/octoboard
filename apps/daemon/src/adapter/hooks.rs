//! What an agent runs or posts for its status hooks: the daemon's hook route as a URL, the shell
//! command that forwards one payload to it, and the per-session script that holds that command.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::shell_quote;

/// Hook timeout given to every agent, in seconds. Short on purpose: Grok defaults `Stop`,
/// `SubagentStop` and `PostToolUse` to 600 s because it treats them as gates, which would stall the
/// UI for ten minutes on a wedged hook, and Codex hard-clamps `SessionEnd` and `Interrupt` to 3 s
/// and prints a visible warning for anything above it.
pub const HOOK_TIMEOUT_SECS: u32 = 3;

/// Where an agent that posts its hooks itself sends them: the daemon's own hook route, which
/// takes the same body the forwarder would have posted. The daemon's key is the URL's query.
pub fn hook_url(session_id: &str, daemon_port: u16, daemon_key: &str) -> String {
    format!("http://127.0.0.1:{daemon_port}/hook/{session_id}?key={daemon_key}")
}

/// The shell command that forwards one hook payload to the daemon: the body of the hook script,
/// and for an agent that runs its hook command through a shell, the whole hook, so the script's
/// own `/bin/sh` is not started on top of it. `exec` rather than a plain call so no shell lingers,
/// and stderr is dropped at the command level as well as inside the binary: a single stderr line
/// is enough to render an error cell in every one of the three agents.
pub fn hook_command(
    self_exe: &str,
    session_id: &str,
    daemon_port: u16,
    daemon_key: &str,
) -> String {
    format!(
        "exec {} hook --session {} --port {} --key {} 2>/dev/null",
        shell_quote(self_exe),
        shell_quote(session_id),
        daemon_port,
        shell_quote(daemon_key)
    )
}

/// Writes the per-session hook script and returns its path. One script per session rather than a
/// shared one, because it carries the session's identity and the daemon's port and key; it is
/// removed with the session's scratch directory.
///
/// The script execs the daemon binary in `hook` mode, which reads the payload, derives the event
/// from it, posts it to the daemon under a hard deadline, and exits 0 whatever happens.
pub fn write_hook_script(
    scratch: &Path,
    self_exe: &str,
    session_id: &str,
    daemon_port: u16,
    daemon_key: &str,
) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let path = hook_script_path(scratch);
    // Owner-only from the first byte: the script carries the key.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o700)
        .open(&path)?;
    writeln!(file, "#!/bin/sh")?;
    writeln!(
        file,
        "{}",
        hook_command(self_exe, session_id, daemon_port, daemon_key)
    )?;
    file.flush()?;
    Ok(path)
}

/// Where a session's hook script is, or would be, in its scratch directory.
pub fn hook_script_path(scratch: &Path) -> PathBuf {
    scratch.join("hook")
}

#[cfg(test)]
mod tests;
