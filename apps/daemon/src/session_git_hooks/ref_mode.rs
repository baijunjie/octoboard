//! `octoboardd git-ref` — what a session's `reference-transaction` hook dispatcher starts, in the
//! background, when a committed transaction of its git has created branches or moved `HEAD`, a
//! local branch or a remote-tracking branch. It posts what it was told to the daemon
//! with the directory git ran the hook in and the `GIT_DIR` it set, from which the daemon finds the
//! repository, and the branch git had checked out, which a branch created from `HEAD` took as its
//! target.
//!
//! The report goes by the launch's route, or by the daemon's where that one is gone
//! ([`super::route`]), so a job a session left running after it ended still reports what it
//! creates.
//!
//! git has already moved on by the time this runs and never sees its outcome, so it exits 0
//! whatever happens and writes nothing anywhere.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;

use super::route::{Deadlines, Routes};

use crate::branch_origin::{CreatedRef, RefReport};
use crate::platform::names;
use crate::time::now_millis;
use crate::wire_path;

/// Hard deadlines for the round trip. Nobody waits on it, so the launch's route is given long
/// enough for Windows to report a loopback port nothing listens on as refused, which is what sends
/// the report on to the daemon's route: measured at 2.0 to 2.5 s, and past 3 s on a loaded host,
/// where a timeout would drop the report instead. The daemon's own route keeps the agent hook
/// forwarder's deadline.
const DEADLINES: Deadlines = Deadlines {
    launch: Duration::from_millis(6000),
    daemon: Duration::from_millis(1500),
};

/// Posts the report and returns. `port` is the launch's, when the session's environment names one;
/// `data` is the data directory of the daemon the report goes to when the launch's route is gone.
/// `head` is the branch the transaction's git had checked out while it was prepared; `moved` says
/// the transaction touched the refs a branch header reads; `refs` alternates a reference name and
/// the object id it was created at, as the dispatcher passes them.
pub fn run(
    session: &str,
    port: Option<u16>,
    data: &Path,
    head: Option<&OsStr>,
    moved: bool,
    refs: &[OsString],
) {
    // Out of the process group of the command whose git started this, which the tool that ran it
    // may kill as soon as that command returns.
    crate::platform::proc::leave_session();
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let git_dir = std::env::var_os("GIT_DIR");
    let report = report_of(
        &cwd,
        git_dir.as_deref(),
        head,
        moved,
        refs,
        now_millis() / 1000,
    );
    let Ok(body) = serde_json::to_vec(&report) else {
        return;
    };
    let _ =
        Routes::from_environment(port, data).post(&format!("/git-ref/{session}"), &body, DEADLINES);
}

/// The report of `refs` made in `cwd` with `git_dir` set and `head` checked out, at `unix_time`.
pub fn report_of(
    cwd: &Path,
    git_dir: Option<&OsStr>,
    head: Option<&OsStr>,
    moved: bool,
    refs: &[OsString],
    unix_time: i64,
) -> RefReport {
    RefReport {
        cwd: wire_path::encode(&names::name_bytes(cwd.as_os_str())),
        git_dir: git_dir.map(|dir| wire_path::encode(&names::name_bytes(dir))),
        head: head.map(|head| wire_path::encode(&names::name_bytes(head))),
        refs: refs
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[name, oid]| CreatedRef {
                name: wire_path::encode(&names::name_bytes(name)),
                oid: oid.to_string_lossy().into_owned(),
            })
            .collect(),
        moved,
        unix_time,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    /// The launch's route outlasts a loaded Windows host's refusal of a closed loopback port
    /// (measured past 3 s), so the report goes on to the daemon's route rather than timing out.
    #[test]
    fn the_launch_route_waits_out_a_slow_refusal() {
        assert!(super::DEADLINES.launch >= Duration::from_secs(6));
    }
}
