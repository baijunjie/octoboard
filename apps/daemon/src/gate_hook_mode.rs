//! `octoboardd gate-hook` — what a session's `reference-transaction` and `pre-push` hook
//! dispatchers start, in the foreground, when git is about to move an existing branch or push
//! one. It asks the daemon whether the revert gate applies to this session's repository and
//! what it needs to know about the branches ([`crate::branch_origin::revert_gate_inputs`]), and
//! runs the gate in this process with the answer, over the hook's own stdin; its exit status is
//! the gate's, so 3 rejects.
//!
//! Anything but an answer to run the gate with lets the update or push through, silently: the
//! daemon not running or not answering in time, a reply it cannot read, and the daemon saying to
//! skip (a console session, another repository, the switch turned off). The gate only ever gets
//! in the way of an update when it has judged that update; a daemon that is down is no reason to
//! stop the user's git.
//!
//! The gate's own `git` calls run in the hook's environment, as the gate expects (see
//! [`crate::revert_gate`]), so this process does not setsid, change directory or touch any
//! variable.

use std::ffi::OsStr;
use std::io::{Cursor, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;

use crate::branch_origin::revert_gate_inputs::GateRequest;
use crate::browse::wire_path;
use crate::revert_gate::{self, GateInputs, Hook};

/// Hard deadline for the round trip, the same as `git-ref`'s and the agent hook forwarder's: git
/// waits on this process for every branch it moves, so a daemon that does not answer promptly
/// must cost a moment, not the user's command.
const TIMEOUT: Duration = Duration::from_millis(1500);

/// Runs the gate for the hook, with stdin, and returns the exit status for the dispatcher.
pub fn run(session: &str, port: u16, hook: &Hook) -> i32 {
    // Read before anything else: whoever feeds the hook's stdin is not left writing into a pipe
    // nobody reads when the update is let through unasked.
    let mut stdin = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut stdin);
    // The user's own request to skip the gate needs no daemon to be honoured.
    if revert_gate::skip_requested() {
        return 0;
    }
    let Ok(cwd) = std::env::current_dir() else {
        return 0;
    };
    let git_dir = std::env::var_os("GIT_DIR");
    let Some(inputs) = inputs_for(port, session, &cwd, git_dir.as_deref()) else {
        return 0;
    };
    revert_gate::check(hook, &inputs, Cursor::new(stdin))
}

/// What the daemon says the gate is to run with for a hook git ran in `cwd` with `git_dir` set;
/// `None` for anything that is not such an answer.
pub(crate) fn inputs_for(
    port: u16,
    session: &str,
    cwd: &Path,
    git_dir: Option<&OsStr>,
) -> Option<GateInputs> {
    let request = GateRequest {
        cwd: wire_path::encode(cwd.as_os_str().as_bytes()),
        git_dir: git_dir.map(|dir| wire_path::encode(dir.as_bytes())),
    };
    let body = serde_json::to_vec(&request).ok()?;
    let reply =
        crate::loopback::post(port, &format!("/revert-gate/{session}"), &body, TIMEOUT).ok()?;
    serde_json::from_slice::<Option<GateInputs>>(&reply).ok()?
}
