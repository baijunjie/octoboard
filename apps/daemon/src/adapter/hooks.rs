//! What an agent runs or posts for its status hooks: the daemon's hook route as a URL, the shell
//! command that forwards one payload to it, and the per-session script that holds that command;
//! and the per-session script an agent's extension starts to reach the session's channel.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::platform::shell::{self, Arg, Dialect, Stderr};

/// Timeout of a command hook, in seconds: the host's start-up allowance for the shell the hook
/// starts. Short on purpose: Grok defaults `Stop`, `SubagentStop` and `PostToolUse` to 600 s because
/// it treats them as gates, which would stall the UI for ten minutes on a wedged hook.
pub use shell::COMMAND_HOOK_TIMEOUT_SECS;

/// Timeout of a hook an agent posts itself, in seconds, the same on every host: no shell starts, so
/// no cold start applies, and the hook is synchronous, so a longer one would stall the turn on a
/// daemon that cannot be reached.
pub const HTTP_HOOK_TIMEOUT_SECS: u32 = 3;

/// Hard deadline for the whole round trip of the `hook` mode (`crate::cli::hook`), posting a hook
/// payload to the daemon. Comfortably inside the hook timeouts the adapters set, so a daemon that
/// is gone or wedged costs the turn a fraction of a second rather than the hook's full timeout. An
/// extension that waits for its hook script to exit waits this long and a little more (Pi's).
pub const HOOK_ROUND_TRIP_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// Where an agent that posts its hooks itself sends them: the daemon's own hook route for this
/// launch (`crate::holder::protocol::hook_path`), which takes the same body the forwarder would
/// have posted. The daemon's key is the URL's query.
pub fn hook_url(session_id: &str, launch_id: &str, daemon_port: u16, daemon_key: &str) -> String {
    format!(
        "http://127.0.0.1:{daemon_port}{}?key={daemon_key}",
        crate::holder::protocol::hook_path(session_id, launch_id)
    )
}

/// The shell command that forwards one hook payload to the daemon: the body of the hook script,
/// and for an agent that runs its hook command through a shell, the whole hook, so the script's
/// own shell is not started on top of it. The shell is replaced by the daemon rather than left
/// waiting for it, and stderr is dropped at the command level as well as inside the binary: a
/// single stderr line is enough to render an error cell in every registered provider.
///
/// `event` is the name of the event the command is registered for, passed on as `--event` so that a
/// payload that names none still has one. An agent that names its event in every payload passes
/// `None`, and one constant command serves all its events; an agent that registers one command per
/// event passes each event's name.
pub fn hook_command(
    self_exe: &str,
    session_id: &str,
    launch_id: &str,
    daemon_port: u16,
    key_file: &Path,
    event: Option<&str>,
) -> String {
    let port = daemon_port.to_string();
    let key_file = key_file.to_string_lossy();
    shell::command_line(
        Path::new(self_exe),
        &hook_args(session_id, launch_id, &port, &key_file, event, None),
        Stderr::Discard,
    )
}

/// [`hook_command`] for an agent whose launch's hooks also run for an agent started inside the
/// session (it names its settings in a variable every process it starts inherits): the hook
/// forwards its payload only when the process that fired it has `mark` on its command line, text
/// the provider knows only the session's own agent process has there (a file of the launch's that
/// it is started with). The mode reads the command lines from the process table, so nothing is
/// started for the check (`crate::cli::hook`). A provider may wrap the command in shells of its
/// own: the mode looks past them. Not for an agent whose own command line carries its hook
/// command or `key_file`'s path.
pub fn hook_command_fired_by(spec: &super::LaunchSpec<'_>, mark: &str, dialect: Dialect) -> String {
    let port = spec.daemon_port.to_string();
    let key_file = spec.key_file.to_string_lossy();
    let exe = Path::new(spec.self_exe);
    let args = hook_args(
        spec.session_id,
        spec.launch_id,
        &port,
        &key_file,
        None,
        Some(mark),
    );
    match dialect {
        Dialect::Posix => shell::command_line(exe, &args, Stderr::Discard),
        Dialect::PowerShell => format!("{} *> $null; exit 0", powershell_invocation(exe, &args)),
    }
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
    shell::command_line_exit_ok(
        Path::new(guard.exe),
        &guard_args(&list, None),
        Stderr::Discard,
    )
}

/// [`guard_command`] for an agent whose launch's hooks also run for an agent started inside the
/// session, as [`hook_command_fired_by`] is for the status hook: the guard decides only when the
/// process that fired it has `mark` on its command line, and lets the call through otherwise. The
/// shells wrapping the command are told by the list's path, which their command lines hold and
/// the agent's does not.
pub fn guard_command_fired_by(
    guard: &super::GuardHook<'_>,
    mark: &str,
    dialect: Dialect,
) -> String {
    let list = guard.list.to_string_lossy();
    let exe = Path::new(guard.exe);
    let args = guard_args(&list, Some(mark));
    match dialect {
        Dialect::Posix => shell::command_line_exit_ok(exe, &args, Stderr::Discard),
        Dialect::PowerShell => format!("{} 2> $null; exit 0", powershell_invocation(exe, &args)),
    }
}

/// The call operator makes a quoted path an invocation. Windows PowerShell accepts literal
/// arguments in single quotes, including paths with spaces and single quotes.
fn powershell_invocation(exe: &Path, args: &[Arg<'_>]) -> String {
    let quote = |value: &str| format!("'{}'", value.replace('\'', "''"));
    let mut command = format!("& {}", quote(&exe.to_string_lossy()));
    for arg in args {
        command.push(' ');
        match arg {
            Arg::Word(word) => command.push_str(word),
            Arg::Value(value) => command.push_str(&quote(value)),
        }
    }
    command
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
        &guard_args(&list, None),
        Stderr::Discard,
    )?)
}

/// The arguments of the daemon's `guard-hook` mode over `list`; `fired_by` adds `--fired-by`, the
/// mark of [`guard_command_fired_by`].
fn guard_args<'a>(list: &'a str, fired_by: Option<&'a str>) -> Vec<Arg<'a>> {
    let mut args = vec![
        Arg::Word("guard-hook"),
        Arg::Word("--list"),
        Arg::Value(list),
    ];
    if let Some(mark) = fired_by {
        args.extend([Arg::Word("--fired-by"), Arg::Value(mark)]);
    }
    args
}

/// Adds the cross-project guard to `hooks`, the `hooks` object of an agent that takes its hooks as
/// JSON (Claude Code and Grok Build): a group of its own in `PreToolUse`, beside the status hooks'
/// group, for the file-edit tools `matcher` names. It is a synchronous command with the guard's
/// timeout and no `failClosed`, so a guard that cannot decide stays silent.
pub fn add_guard_group(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    event: &str,
    matcher: &str,
    command: &str,
) {
    hooks
        .get_mut(event)
        .and_then(serde_json::Value::as_array_mut)
        .expect("guard event is registered")
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
/// machine. `event` adds `--event`, which the mode posts as the hook route's `event` parameter,
/// and `fired_by` adds `--fired-by`, the mark of [`hook_command_fired_by`].
fn hook_args<'a>(
    session_id: &'a str,
    launch_id: &'a str,
    port: &'a str,
    key_file: &'a str,
    event: Option<&'a str>,
    fired_by: Option<&'a str>,
) -> Vec<Arg<'a>> {
    let mut args = vec![
        Arg::Word("hook"),
        Arg::Word("--session"),
        Arg::Value(session_id),
        Arg::Word("--launch"),
        Arg::Value(launch_id),
        Arg::Word("--port"),
        Arg::Word(port),
        Arg::Word("--key-file"),
        Arg::Value(key_file),
    ];
    if let Some(event) = event {
        args.extend([Arg::Word("--event"), Arg::Value(event)]);
    }
    if let Some(mark) = fired_by {
        args.extend([Arg::Word("--fired-by"), Arg::Value(mark)]);
    }
    args
}

/// Writes the per-session hook script and returns its path. One script per session rather than a
/// shared one, because it carries the session's identity and its launch's, the daemon's port and
/// `key_file`, the file holding its key; it is removed with the session's scratch directory.
///
/// The script execs the daemon binary in `hook` mode, which reads the payload, derives the event
/// from it, posts it to the daemon under a hard deadline, and exits 0 whatever happens. It is
/// readable by its owner alone from the first byte, as the key file it names is.
///
/// `event` is what [`hook_command`] takes it for: the script then passes `--event`, and lives at
/// [`hook_event_script_path`], so that the scripts of one session's events do not overwrite each
/// other. With `None` it is the session's one script, at [`hook_script_path`].
pub fn write_hook_script(
    scratch: &Path,
    self_exe: &str,
    session_id: &str,
    launch_id: &str,
    daemon_port: u16,
    key_file: &Path,
    event: Option<&str>,
) -> Result<PathBuf> {
    let port = daemon_port.to_string();
    let key_file = key_file.to_string_lossy();
    Ok(shell::write_runnable(
        &event.map_or_else(
            || hook_script_path(scratch),
            |event| hook_event_script_path(scratch, event),
        ),
        Path::new(self_exe),
        &hook_args(session_id, launch_id, &port, &key_file, event, None),
        Stderr::Discard,
    )?)
}

/// Where a session's hook script is, or would be, in its scratch directory, for an agent whose one
/// script serves every event.
pub fn hook_script_path(scratch: &Path) -> PathBuf {
    scratch.join("hook")
}

/// Where the hook script of one event is, or would be, in the scratch directory: `hook-<event>`,
/// with every character of the name that is not a letter, a digit, `-` or `_` replaced by `_`, so
/// that it is a safe file name. Names that differ only in such characters share a script, which
/// is harmless while they are not registered for one session.
pub fn hook_event_script_path(scratch: &Path, event: &str) -> PathBuf {
    let name: String = event
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    scratch.join(format!("hook-{name}"))
}

#[cfg(test)]
mod tests;
