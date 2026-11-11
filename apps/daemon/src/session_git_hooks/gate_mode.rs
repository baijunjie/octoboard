//! `octoboardd gate-hook` — what a session's `reference-transaction` and `pre-push` hook
//! dispatchers start, in the foreground, when git is about to move an existing branch or push
//! one. It asks the daemon whether the revert gate applies to this session's repository and
//! what it needs to know about the branches ([`super::gate_inputs`]), and runs the gate in this
//! process with the answer, over the hook's own stdin; its exit status is the gate's, so 3 rejects.
//!
//! The question goes by the launch's route, or by the daemon's where that one is gone
//! ([`super::route`]), so a job a session left running after it ended is still gated.
//!
//! Anything but an answer to run the gate with lets the update or push through, silently: no
//! route to a daemon, the daemon not running or not answering in time, a reply it cannot read, and
//! the daemon saying to skip (a console session, another repository, the switch turned off). A
//! daemon that answers with an error status is let through with a line on stderr saying so, since
//! that is a mismatch or a fault to be looked into, not a daemon being down. The gate only ever
//! gets in the way of an update when it has judged that update; a daemon that is down is no reason
//! to stop the user's git.
//!
//! The gate's own `git` calls run in the hook's environment, as the gate expects (see
//! [`crate::revert_gate`]), so this process does not setsid, change directory or touch any
//! variable.

use std::ffi::OsStr;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::time::Duration;

use crate::holder::RELAY_DEADLINE;
use crate::platform::names;
use crate::revert_gate::{self, GateInputs, Hook};
use crate::wire_path;

use super::gate_inputs::GateRequest;
use super::route::{Deadlines, Outcome, Routes};

/// Hard deadlines for the round trip: git waits on this process for every branch it moves. On the
/// launch's route the call goes to the session's holder, which holds it for up to
/// [`RELAY_DEADLINE`] while no daemon is linked (one that is starting or restarting), so the
/// deadline is that plus a little for the daemon to answer once it is. The daemon's own route holds
/// nothing back, so a daemon that does not answer within the little is not waited for. A gate that
/// cannot be asked within its deadline lets the update through.
pub(crate) const DEADLINES: Deadlines = Deadlines {
    launch: RELAY_DEADLINE.saturating_add(Duration::from_millis(2500)),
    daemon: Duration::from_millis(2500),
};

/// Runs the gate for the hook, with stdin, and returns the exit status for the dispatcher. `port`
/// is the launch's, when the session's environment names one; `data` is the data directory of the
/// daemon asked when the launch's route is gone.
pub fn run(session: &str, port: Option<u16>, data: &Path, hook: &Hook) -> i32 {
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
    let routes = Routes::from_environment(port, data);
    let Some(inputs) = inputs_for(&routes, session, &cwd, git_dir.as_deref(), DEADLINES) else {
        return 0;
    };
    revert_gate::check(hook, &inputs, Cursor::new(stdin))
}

/// What the daemon says the gate is to run with for a hook git ran in `cwd` with `git_dir` set;
/// `None` for anything that is not such an answer.
pub(crate) fn inputs_for(
    routes: &Routes,
    session: &str,
    cwd: &Path,
    git_dir: Option<&OsStr>,
    deadlines: Deadlines,
) -> Option<GateInputs> {
    let request = GateRequest {
        cwd: wire_path::encode(&names::name_bytes(cwd.as_os_str())),
        git_dir: git_dir.map(|dir| wire_path::encode(&names::name_bytes(dir))),
    };
    let body = serde_json::to_vec(&request).ok()?;
    let reply = match routes.post(&format!("/revert-gate/{session}"), &body, deadlines) {
        Outcome::Answered(reply) => reply,
        Outcome::Unrouted => return None,
        Outcome::Failed(err) => {
            // A daemon that answered with an error is not one that is down: the update is still
            // let through, but not without saying that the gate did not look at it. A holder
            // saying it has no daemon, or lost it mid-call, is a daemon that is down.
            if crate::loopback::error_status(&err).is_some()
                && !crate::loopback::is_no_daemon(&err)
                && !crate::loopback::is_daemon_lost(&err)
            {
                let _ = writeln!(
                    std::io::stderr(),
                    "octoboard: the revert gate did not check this update: {err}"
                );
            }
            return None;
        }
    };
    serde_json::from_slice::<Option<GateInputs>>(&reply).ok()?
}
