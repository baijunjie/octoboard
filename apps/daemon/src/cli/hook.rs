//! `octoboardd hook` — what the per-session hook script execs, and what Claude Code's command hook
//! execs directly. It reads the agent's hook payload from stdin and posts it to the daemon. A
//! Claude Code that takes HTTP hooks posts to the daemon itself and never runs this.
//!
//! Its only hard requirement is to be invisible when anything goes wrong: every one of the three
//! agents renders a failing hook to the user (an error block on Claude Code, a scrollback line on
//! Grok, a history cell on Codex), so this exits 0 unconditionally and writes nothing to stdout or
//! stderr. Stdout silence also matters for correctness, not just cosmetics: a `PreToolUse` hook on
//! Grok that emits JSON can deny a tool call, and on Claude Code stdout can steer the turn.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// Hard deadline for the whole round trip. Comfortably inside the hook timeouts the adapters set,
/// so a daemon that is gone or wedged costs the turn a fraction of a second rather than the hook's
/// full timeout.
const TIMEOUT: Duration = Duration::from_millis(1500);

/// Runs the hook and returns. Never fails: the caller exits 0 regardless. A key file that cannot
/// be read is a daemon that refuses the post: nothing is sent.
pub fn run(session: &str, port: u16, key_file: &Path) {
    // The key first: the file goes with the session's scratch directory when the agent exits, and
    // a hook the agent does not wait for can start just before that.
    let key = crate::key_file::read(key_file);
    let mut payload = Vec::new();
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return;
    }
    let Some(key) = key else {
        return;
    };
    // The response is discarded: the daemon never steers the agent through it.
    let _ = crate::loopback::post(port, &key, &format!("/hook/{session}"), &payload, TIMEOUT);
}
