//! Capturing the user's real shell environment, which every agent is then launched with.
//!
//! `$SHELL -l -c ...` is not enough: tool paths such as `grok`, `codex` and a node-version
//! manager's shims are set up in `~/.zshrc`, which a login-only non-interactive zsh never sources.
//! The working form is `$SHELL -l -i -c 'env -0'`; forcing `TERM=dumb` and feeding `/dev/null` on
//! stdin keeps that interactive shell from printing a prompt or reading a stray tty.
//!
//! That `TERM` is for the snapshot shell only and must not reach the agent: `env -0` dumps it like
//! any other variable, and an agent inheriting `TERM=dumb` downgrades its own renderer, colour
//! handling and mouse reporting. `SNAPSHOT_ONLY_VARS` is dropped from the snapshot and the PTY
//! command sets its own `TERM` instead.
//!
//! The snapshot is taken per launch rather than cached: a node-version manager's `PATH` entry can
//! point at a per-shell-instance directory, and a long-running daemon would otherwise never pick
//! up an edit the user makes to their shell configuration. Agent launches are human-paced, so the
//! extra `fork+exec` is immaterial.

use std::collections::HashMap;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Variables that belong to the snapshot shell rather than to the agent, and are stripped from the
/// snapshot before it is handed to a PTY command.
pub const SNAPSHOT_ONLY_VARS: &[&str] = &[
    "TERM",
    "TERMINFO",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
];

/// Variables that mark *this process* as running inside an agent session. A daemon started from
/// within such a session would otherwise hand them to every agent it spawns, and the agent takes
/// them at face value: Claude Code turns transcript saving off and stops applying
/// `--permission-mode`, with no error anywhere.
///
/// Enumerated, because prefix matching is the wrong axis and fails in both directions. It strips
/// too much — `GROK_CODE_XAI_API_KEY`, `GROK_HOME`, `CODEX_HOME` and `CLAUDE_CODE_USE_BEDROCK` are
/// user settings, and dropping the first of those stops a key-authenticated session from starting
/// at all, with a failure that looks like a login problem. And it strips too little — a Claude Code
/// session also exports `CLAUDE_PID` and `CLAUDE_EFFORT`, which match neither `CLAUDECODE` nor
/// `CLAUDE_CODE_`. The two sets are not separable by name, so this list is what was observed inside
/// live sessions; extend it the same way, by dumping `env` inside a session rather than by guessing
/// a pattern.
///
/// Stripping these does not disturb authentication: credentials live in the macOS Keychain, so
/// `claude auth status` still reports a logged-in account with the markers removed. A login failure
/// is the obvious thing to blame this filter for, and re-adding the variables to "fix" it would
/// reintroduce the original bug.
pub const INHERITED_SESSION_VARS: &[&str] = &[
    "AI_AGENT",
    "CLAUDECODE",
    "CLAUDE_CODE_BRIDGE_MCP_CARRIER",
    "CLAUDE_CODE_BRIDGE_OWNER_ACCOUNT_UUID",
    "CLAUDE_CODE_BRIDGE_OWNER_ORG_UUID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_ENVIRONMENT_KIND",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_CODE_WORKER_EPOCH",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "CLAUDE_PROJECT_DIR",
    "GROK_HOOK_EVENT",
    "GROK_HOOK_NAME",
    "GROK_SESSION_ID",
    "GROK_WORKSPACE_ROOT",
];

/// True when a variable marks the daemon's own agent session rather than the user's environment.
pub fn is_filtered(key: &str) -> bool {
    SNAPSHOT_ONLY_VARS.contains(&key) || INHERITED_SESSION_VARS.contains(&key)
}

pub fn shell_path() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

/// Runs `$SHELL -l -i -c 'env -0'` and returns the parsed environment with the filtered variables
/// already removed.
pub fn snapshot() -> Result<HashMap<String, String>> {
    let shell = shell_path();
    let output = Command::new(&shell)
        .args(["-l", "-i", "-c", "env -0"])
        .env("TERM", "dumb")
        .stdin(std::process::Stdio::null())
        .output()
        .with_context(|| {
            format!("spawning `{shell} -l -i -c 'env -0'` to snapshot the environment")
        })?;

    if !output.status.success() {
        bail!(
            "`{shell} -l -i -c 'env -0'` exited with {:?}; stderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut env = HashMap::new();
    for pair in output.stdout.split(|&b| b == 0) {
        if pair.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(pair);
        if let Some((key, value)) = text.split_once('=') {
            if is_filtered(key) {
                continue;
            }
            env.insert(key.to_string(), value.to_string());
        }
    }
    Ok(env)
}

/// Resolves a bare binary name to an absolute path by searching the snapshot's `PATH`, mirroring
/// what the login shell would have found. Deliberately does not leave resolution to
/// `portable_pty`, whose own search tries `cwd.join(exe)` first and accepts it on `.exists()`
/// alone — so a project file named `claude` would silently shadow the real CLI.
pub fn resolve_binary(name: &str, env: &HashMap<String, String>) -> Result<String> {
    use std::os::unix::fs::PermissionsExt;

    let path_var = env.get("PATH").map(String::as_str).unwrap_or_default();
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = std::path::Path::new(dir).join(name);
        let executable = candidate
            .metadata()
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
        if executable {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    bail!("`{name}` was not found on PATH in the snapshotted shell environment")
}
