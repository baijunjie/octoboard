//! `octoboardd hook` — what the per-session hook script execs. It reads the agent's hook payload
//! from stdin and posts it to the daemon.
//!
//! Its only hard requirement is to be invisible when anything goes wrong: every one of the three
//! agents renders a failing hook to the user (an error block on Claude Code, a scrollback line on
//! Grok, a history cell on Codex), so this exits 0 unconditionally and writes nothing to stdout or
//! stderr. Stdout silence also matters for correctness, not just cosmetics: a `PreToolUse` hook on
//! Grok that emits JSON can deny a tool call, and on Claude Code stdout can steer the turn.

use std::io::Read;
use std::time::Duration;

/// Hard deadline for the whole round trip. Comfortably inside the hook timeouts the adapters set,
/// so a daemon that is gone or wedged costs the turn a fraction of a second rather than the hook's
/// full timeout.
const TIMEOUT: Duration = Duration::from_millis(1500);

/// Runs the hook and returns. Never fails: the caller exits 0 regardless.
pub fn run(session: &str, port: u16) {
    let mut payload = Vec::new();
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return;
    }
    // The response is discarded: the daemon never steers the agent through it.
    let _ = crate::loopback::post(port, &format!("/hook/{session}"), &payload, TIMEOUT);
}
