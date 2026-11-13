//! Running a build with `--version`, to learn that it starts on this machine and which version it
//! is, before an update puts it in place and when a rollback names the build it goes back to.
//!
//! `octoboardd --version` prints `octoboardd <version>` and exits 0, touching nothing (no data
//! directory, no lock, no network): clap answers it while parsing the arguments, before anything
//! else runs. A build older than the flag rejects it as an unknown argument and starts nothing.

use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::platform::proc::Reach;
use crate::processes::subprocess::{run_bounded, BoundedFailure, Bounds};

/// How long a build may take to answer `--version`. Far above a real build's few milliseconds; a
/// cold disk or a loaded machine is not to fail an update.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// The most of the answer that is read, and of what it says on stderr.
const ANSWER_LIMIT: usize = 4096;

/// How often a run that finds the file busy is tried again, and how long apart. A file just
/// written is busy (`ETXTBSY`) while a process forked elsewhere in this one still holds the
/// descriptor it was written through, until that process's own `exec` closes it.
const BUSY_TRIES: u32 = 20;
const BUSY_WAIT: Duration = Duration::from_millis(50);

/// Runs `exe --version` and checks that it says `version`. The error says, in English, why the
/// build is not taken to start: it could not be run, it did not answer within the time, it failed,
/// or it named another version. Blocking.
pub fn expect_version(exe: &Path, version: &str) -> Result<(), String> {
    let said = reported_version(exe, PROBE_TIMEOUT)?;
    if said != version {
        return Err(format!(
            "the new build says it is version {said}, not the {version} the release names"
        ));
    }
    Ok(())
}

/// The version `exe --version` reports: the last word of the first line of what it prints. The
/// error says, in English, why there is none. Blocking, for at most about `timeout`.
pub fn reported_version(exe: &Path, timeout: Duration) -> Result<String, String> {
    let output = run(exe, timeout)?;
    if !output.status.success() {
        return Err(format!(
            "`{} --version` failed ({})",
            exe.display(),
            output.status
        ));
    }
    parse_answer(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| format!("`{} --version` named no version", exe.display()))
}

/// The version in an answer to `--version`: the last word of its first line, when that reads as a
/// version.
pub(super) fn parse_answer(answer: &str) -> Option<String> {
    let word = answer.lines().next()?.split_whitespace().last()?;
    crate::holder::compat::Version::parse(word).map(|_| word.to_owned())
}

/// Runs `exe --version` within bounds (`run_bounded`: nothing on its stdin, its process group
/// killed past `timeout`), from the root directory so it pins no mount, trying again while the
/// file is busy.
fn run(exe: &Path, timeout: Duration) -> Result<Output, String> {
    let bounds = Bounds {
        timeout,
        stdout_limit: ANSWER_LIMIT,
        stderr_limit: ANSWER_LIMIT,
        reach: Reach::Group,
        term_grace: Duration::ZERO,
    };
    let mut tries = 0;
    loop {
        let mut command = Command::new(exe);
        command.arg("--version").current_dir("/");
        match run_bounded(&mut command, &bounds, &AtomicBool::new(false)) {
            Ok(output) => return Ok(output),
            Err(BoundedFailure::Failed(err)) if is_busy(&err) && tries < BUSY_TRIES => {
                tries += 1;
                std::thread::sleep(BUSY_WAIT);
            }
            Err(BoundedFailure::TimedOut(_)) => {
                return Err(format!(
                    "`{} --version` did not answer within {} seconds",
                    exe.display(),
                    timeout.as_secs()
                ))
            }
            Err(BoundedFailure::Failed(err)) => {
                return Err(format!("`{}` could not be run: {err:#}", exe.display()))
            }
            Err(other) => return Err(format!("`{} --version` failed: {other}", exe.display())),
        }
    }
}

/// Whether a failure to run was the file being busy (`ETXTBSY`).
fn is_busy(err: &anyhow::Error) -> bool {
    #[cfg(unix)]
    {
        err.chain()
            .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
            .any(|io| io.raw_os_error() == Some(libc::ETXTBSY))
    }
    #[cfg(not(unix))]
    {
        let _ = err;
        false
    }
}
