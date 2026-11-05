//! The helper processes the platform's contract tests start: this test binary run again with only
//! one helper test selected, which reads what to do from its environment and is a no-op in an
//! ordinary test run. No contract test needs a program of the host's own.

use std::ffi::OsString;
use std::path::Path;

/// The environment variable that turns a helper test from a no-op into the helper a test starts,
/// naming what it does.
pub(crate) const ROLE: &str = "OCTOBOARD_TEST_PROC_ROLE";
/// Where the helper writes its pid, and with it that it is ready, or, for a leader, its helper's.
pub(crate) const PID_FILE: &str = "OCTOBOARD_TEST_PROC_PID_FILE";

/// The program a helper is started as: this test binary.
pub(crate) fn program() -> std::path::PathBuf {
    std::env::current_exe().expect("the test binary")
}

/// The arguments that run the helper test `helper` (its full path, as `--exact` takes it) alone.
pub(crate) fn args(helper: &str) -> [OsString; 3] {
    ["--exact".into(), helper.into(), "--nocapture".into()]
}

/// Writes `pid` to `path` whole, through a rename, so that `wait_for_pid` never reads half of it.
pub(crate) fn write_pid(path: &Path, pid: u32) {
    let partial = path.with_extension("partial");
    std::fs::write(&partial, pid.to_string()).expect("the pid is written");
    std::fs::rename(partial, path).expect("the pid is in place");
}
