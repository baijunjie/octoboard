//! `octoboardd git-ref` — what a session's `reference-transaction` hook dispatcher starts, in the
//! background, when a transaction of its git has created branches. It posts them to the daemon
//! with the directory git ran the hook in and the `GIT_DIR` it set, from which the daemon finds the
//! repository.
//!
//! git has already moved on by the time this runs and never sees its outcome, so it exits 0
//! whatever happens and writes nothing anywhere.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;

use crate::branch_origin::{CreatedRef, RefReport};
use crate::browse::wire_path;
use crate::protocol::now_millis;

/// Hard deadline for the round trip, the same as the agent hook forwarder's.
const TIMEOUT: Duration = Duration::from_millis(1500);

/// Posts the report and returns. `refs` alternates a reference name and the object id it was
/// created at, as the dispatcher passes them.
pub fn run(session: &str, port: u16, refs: &[OsString]) {
    // Out of the process group of the command whose git started this, which the tool that ran it
    // may kill as soon as that command returns.
    // SAFETY: no arguments, and it only changes this process's own session.
    unsafe {
        libc::setsid();
    }
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let git_dir = std::env::var_os("GIT_DIR");
    let report = report_of(&cwd, git_dir.as_deref(), refs, now_millis() / 1000);
    let Ok(body) = serde_json::to_vec(&report) else {
        return;
    };
    let _ = crate::loopback::post(port, &format!("/git-ref/{session}"), &body, TIMEOUT);
}

/// The report of `refs` made in `cwd` with `git_dir` set, at `unix_time`.
pub fn report_of(
    cwd: &Path,
    git_dir: Option<&OsStr>,
    refs: &[OsString],
    unix_time: i64,
) -> RefReport {
    RefReport {
        cwd: wire_path::encode(cwd.as_os_str().as_bytes()),
        git_dir: git_dir.map(|dir| wire_path::encode(dir.as_bytes())),
        refs: refs
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[name, oid]| CreatedRef {
                name: wire_path::encode(name.as_bytes()),
                oid: oid.to_string_lossy().into_owned(),
            })
            .collect(),
        unix_time,
    }
}
