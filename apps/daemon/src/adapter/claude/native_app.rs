//! Opening a Claude Code conversation in the Claude application: the CLI does it itself, as
//! `claude --desktop --resume <id>`, which hands the conversation to the application and exits.
//! It runs as the session's account, in the session's directory, as the agent is launched, and the
//! application it opens is left running when the CLI is gone.

use std::process::Output;
use std::time::Duration;

use crate::adapter::account_cli::{admitted_command, failure_detail, NotRun};
use crate::adapter::{native_app_unavailable as unavailable, NativeApp, NativeAppOpen};

/// How long `claude --desktop` may take to hand the conversation over and exit.
const OPEN_TIMEOUT: Duration = Duration::from_secs(30);

/// The most of the end of `claude --desktop`'s output that is read, for the reason a failure gives.
const OUTPUT_LIMIT: usize = 16 * 1024;

/// The scratch file `claude --desktop`'s output is written to.
const LOG_FILE: &str = "open-in-app.log";

/// The last `limit` bytes of the file `path`, nothing when it cannot be read.
fn tail_of(path: &std::path::Path, limit: usize) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = crate::hostfs::open_regular(path) else {
        return Vec::new();
    };
    let length = file.metadata().map_or(0, |meta| meta.len());
    let mut tail = Vec::new();
    if file
        .seek(SeekFrom::Start(length.saturating_sub(limit as u64)))
        .is_ok()
    {
        let _ = file.take(limit as u64).read_to_end(&mut tail);
    }
    tail
}

pub(super) struct ClaudeApp;

impl NativeApp for ClaudeApp {
    fn open(&self, open: &NativeAppOpen<'_>) -> anyhow::Result<()> {
        let config_dir = open.session.config_dir.as_deref().map(std::path::Path::new);
        let mut command = admitted_command(
            open.session.agent,
            open.shell,
            config_dir,
            &["--desktop", "--resume", open.conversation],
        )
        .map_err(|not_run| match not_run {
            NotRun::BinaryRefused(refusal) => anyhow::Error::new(refusal),
            other => unavailable(&other.detail()),
        })?;
        command.command.current_dir(open.cwd);
        // The application must outlive the CLI, which hands the conversation over and exits, so the
        // run reaches only the CLI's own group, and its output goes to a file that nothing the
        // application started can hold open: one file for the session, written over by the next run
        // and removed after this one as far as the host lets it be.
        std::fs::create_dir_all(open.scratch).map_err(|err| unavailable(&err.to_string()))?;
        let log = open.scratch.join(LOG_FILE);
        let ran = command.run_leaving_descendants(&log, OPEN_TIMEOUT);
        let printed = tail_of(&log, OUTPUT_LIMIT);
        let _ = std::fs::remove_file(&log);
        let status = ran.map_err(|failure| unavailable(&failure.to_string()))?;
        if status.success() {
            return Ok(());
        }
        let detail = failure_detail(
            &Output {
                status,
                stdout: printed,
                stderr: Vec::new(),
            },
            |line| line,
        );
        Err(unavailable(&if detail.is_empty() {
            format!("`claude --desktop` exited with {status}")
        } else {
            detail
        }))
    }
}

#[cfg(test)]
mod tests;
