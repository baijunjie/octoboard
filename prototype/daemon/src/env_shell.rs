//! Capturing the user's real shell environment, and launching agent binaries with it.
//!
//! Verified fact (do not re-derive): `$SHELL -l -c ...` is not enough — tool paths such as
//! `grok` and `node` (via fnm) are set up in `~/.zshrc`, which a login-only non-interactive zsh
//! does not source. The working form is `$SHELL -l -i -c ...`; forcing TERM=dumb and feeding
//! /dev/null on stdin keeps that interactive shell from printing a prompt or reading a stray
//! tty, which is what made the earlier manual verification noise-free.
//!
//! That TERM is for the snapshot shell only and must not reach the agent: `env -0` dumps it like
//! any other variable, and an agent inheriting `TERM=dumb` downgrades itself — it is a plausible
//! cause of Claude Code falling back to its non-fullscreen renderer, and it also drives colour,
//! mouse-reporting and alt-screen detection. `SNAPSHOT_ONLY_VARS` is dropped from the snapshot and
//! the PTY command sets its own TERM instead.
//!
//! Decision: we re-run the snapshot before every launch rather
//! than caching it once at startup. A long-running daemon started once from Finder would
//! otherwise never pick up a `~/.zshrc` edit the user makes mid-session, and agent launches are
//! human-paced (one every few seconds at most), so the extra `fork+exec` of a login shell is
//! immaterial. This also means no `--refresh-env` flag is needed — every launch already
//! refreshes. `--login-shell-launch` (see the CLI in `main.rs`) is the kept-around fallback that skips
//! this snapshot entirely and runs the agent through `$SHELL -l -i -c '<command>'` directly.

use std::collections::HashMap;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Variables that belong to the snapshot shell rather than to the agent, and are stripped from
/// the snapshot before it is handed to a PTY command.
pub const SNAPSHOT_ONLY_VARS: &[&str] = &[
    "TERM",
    "TERMINFO",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
];

/// Variables that mark *this process* as running inside an agent session, enumerated rather than
/// prefix-matched. A daemon started from within such a session (which is how this prototype is
/// usually run) would otherwise hand them to every agent it spawns, and the agent takes them at
/// face value: Claude Code turns transcript saving off, reports `CLAUDE_CODE_ENTRYPOINT=sdk-cli`,
/// and stops applying `--permission-mode`.
///
/// Enumerated, because prefix matching is the wrong axis and fails in both directions. It strips
/// too much — `GROK_CODE_XAI_API_KEY`, `GROK_HOME`, `CODEX_HOME` and `CLAUDE_CODE_USE_BEDROCK` are
/// user settings, and dropping the first of those stops a key-authenticated session from starting
/// at all, with a failure that looks like a login problem. And it strips too little — a Claude Code
/// session also exports `CLAUDE_PID` and `CLAUDE_EFFORT`, which match neither `CLAUDECODE` nor
/// `CLAUDE_CODE_`. The two sets are not separable by name, so the list below is what was observed
/// inside live sessions; extend it the same way, by dumping `env` inside a session rather than by
/// guessing a pattern.
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
pub fn is_inherited_session_var(key: &str) -> bool {
    INHERITED_SESSION_VARS.contains(&key)
}

pub fn shell_path() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

/// Runs `$SHELL -l -i -c 'env -0'` and parses the NUL-separated `KEY=VALUE` output.
pub fn snapshot_shell_env() -> Result<HashMap<String, String>> {
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
        if let Some((k, v)) = text.split_once('=') {
            env.insert(k.to_string(), v.to_string());
        }
    }
    Ok(env)
}
