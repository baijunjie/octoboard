//! What an agent runs or posts for its status hooks: the daemon's hook route as a URL, the shell
//! command that forwards one payload to it, and the per-session script that holds that command.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::platform::shell::{self, Arg, Stderr};

/// Timeout of a command hook, in seconds: the host's start-up allowance for the shell the hook
/// starts. Short on purpose: Grok defaults `Stop`, `SubagentStop` and `PostToolUse` to 600 s because
/// it treats them as gates, which would stall the UI for ten minutes on a wedged hook.
pub use shell::COMMAND_HOOK_TIMEOUT_SECS;

/// Timeout of a hook an agent posts itself, in seconds, the same on every host: no shell starts, so
/// no cold start applies, and the hook is synchronous, so a longer one would stall the turn on a
/// daemon that cannot be reached.
pub const HTTP_HOOK_TIMEOUT_SECS: u32 = 3;

/// Where an agent that posts its hooks itself sends them: the daemon's own hook route, which
/// takes the same body the forwarder would have posted. The daemon's key is the URL's query.
pub fn hook_url(session_id: &str, daemon_port: u16, daemon_key: &str) -> String {
    format!("http://127.0.0.1:{daemon_port}/hook/{session_id}?key={daemon_key}")
}

/// The shell command that forwards one hook payload to the daemon: the body of the hook script,
/// and for an agent that runs its hook command through a shell, the whole hook, so the script's
/// own shell is not started on top of it. The shell is replaced by the daemon rather than left
/// waiting for it, and stderr is dropped at the command level as well as inside the binary: a
/// single stderr line is enough to render an error cell in every registered provider.
pub fn hook_command(self_exe: &str, session_id: &str, daemon_port: u16, key_file: &Path) -> String {
    let port = daemon_port.to_string();
    let key_file = key_file.to_string_lossy();
    shell::command_line(
        Path::new(self_exe),
        &hook_args(session_id, &port, &key_file),
        Stderr::Discard,
    )
}

/// Timeout of the cross-project guard's hook, in seconds, on every host. The hook is synchronous,
/// so an agent's turn waits for it, and its first run of a launch may be a cold start of a shell
/// and the daemon's executable, which can outlast [`COMMAND_HOOK_TIMEOUT_SECS`] of a warm host. A
/// guard that is killed lets the call through.
pub const GUARD_HOOK_TIMEOUT_SECS: u32 = 10;

/// The shell command of the cross-project guard's hook for an agent that runs a command through a
/// POSIX shell (Claude Code, through Git Bash on Windows): the daemon's `guard-hook` mode over the
/// launch's list, followed by an exit with status 0 whatever it did, since an agent reads exit
/// status 2, which a command line the mode cannot parse gives, as a block. Stderr is dropped for
/// the reason [`hook_command`] gives, and stdout is the agent's to read as a decision.
pub fn guard_command(guard: &super::GuardHook<'_>) -> String {
    let list = guard.list.to_string_lossy();
    shell::command_line_exit_ok(Path::new(guard.exe), &guard_args(&list), Stderr::Discard)
}

/// Writes the per-session guard script, which does what [`guard_command`] does, and returns its
/// path to register as the command of an agent that runs a hook by its script's path (Codex and
/// Grok Build), as it does for the status hook: a command line written in one shell's language is
/// not one every host's shell runs.
pub fn write_guard_script(scratch: &Path, exe: &str, list: &Path) -> Result<PathBuf> {
    let list = list.to_string_lossy();
    Ok(shell::write_runnable_exit_ok(
        &scratch.join("guard-hook"),
        Path::new(exe),
        &guard_args(&list),
        Stderr::Discard,
    )?)
}

fn guard_args(list: &str) -> [Arg<'_>; 3] {
    [
        Arg::Word("guard-hook"),
        Arg::Word("--list"),
        Arg::Value(list),
    ]
}

/// Adds the cross-project guard to `hooks`, the `hooks` object of an agent that takes its hooks as
/// JSON (Claude Code and Grok Build): a group of its own in `PreToolUse`, beside the status hooks'
/// group, for the file-edit tools `matcher` names. It is a synchronous command with the guard's
/// timeout and no `failClosed`, so a guard that cannot decide stays silent.
pub fn add_guard_group(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    matcher: &str,
    command: &str,
) {
    hooks
        .get_mut("PreToolUse")
        .and_then(serde_json::Value::as_array_mut)
        .expect("PreToolUse is registered")
        .push(serde_json::json!({
            "matcher": matcher,
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": GUARD_HOOK_TIMEOUT_SECS,
            }],
        }));
}

/// The arguments of the daemon's `hook` mode, which every hook command and script runs. The key is
/// named by the file that holds it, never given: arguments can be listed by every user of the
/// machine.
fn hook_args<'a>(session_id: &'a str, port: &'a str, key_file: &'a str) -> [Arg<'a>; 7] {
    [
        Arg::Word("hook"),
        Arg::Word("--session"),
        Arg::Value(session_id),
        Arg::Word("--port"),
        Arg::Word(port),
        Arg::Word("--key-file"),
        Arg::Value(key_file),
    ]
}

/// Writes the per-session hook script and returns its path. One script per session rather than a
/// shared one, because it carries the session's identity, the daemon's port and `key_file`, the
/// file holding its key; it is removed with the session's scratch directory.
///
/// The script execs the daemon binary in `hook` mode, which reads the payload, derives the event
/// from it, posts it to the daemon under a hard deadline, and exits 0 whatever happens. It is
/// readable by its owner alone from the first byte, as the key file it names is.
pub fn write_hook_script(
    scratch: &Path,
    self_exe: &str,
    session_id: &str,
    daemon_port: u16,
    key_file: &Path,
) -> Result<PathBuf> {
    let port = daemon_port.to_string();
    let key_file = key_file.to_string_lossy();
    Ok(shell::write_runnable(
        &hook_script_path(scratch),
        Path::new(self_exe),
        &hook_args(session_id, &port, &key_file),
        Stderr::Discard,
    )?)
}

/// Where a session's hook script is, or would be, in its scratch directory.
pub fn hook_script_path(scratch: &Path) -> PathBuf {
    scratch.join("hook")
}

#[cfg(test)]
mod tests;
