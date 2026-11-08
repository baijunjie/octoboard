//! An agent's CLI run for an account in the background, with no terminal: the [`Process`] it runs
//! in, killed with its whole session when it is ended early, the [`Line`]s it says, and the
//! [`SigningIn`] count that keeps the account's sharing from changing under it. Used by an
//! account's own login (`accounts::login::flows`) and by an MCP server's sign-in and sign-out
//! (`accounts::mcp_sign_in`).

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;

use crate::platform::proc::{self, ChildHandle, ProcessId, Reach};
use crate::state::AppState;
use crate::terminal::plain_text::strip_escapes;

/// How long a CLI's last words are waited for after it exits.
const LAST_WORDS: Duration = Duration::from_millis(300);

/// How often a CLI that has been killed is looked at for having exited, which is found out
/// without reaping it so that what it left in its session can be killed first (see
/// [`Process::stop`]).
const EXIT_POLL: Duration = Duration::from_millis(20);

/// A sign-in counted under its account's directory, which refuses a change of the account's
/// sharing; every client is told when it starts and when it ends. An account's login and an MCP
/// server's sign-in or sign-out are counted alike.
pub(crate) struct SigningIn {
    state: Arc<AppState>,
    run: Option<crate::accounts::shared_config::Run>,
}

impl SigningIn {
    pub(crate) fn new(state: Arc<AppState>, run: crate::accounts::shared_config::Run) -> Self {
        state.publish_account_sharing();
        Self {
            state,
            run: Some(run),
        }
    }
}

impl Drop for SigningIn {
    fn drop(&mut self) {
        drop(self.run.take());
        self.state.publish_account_sharing();
    }
}

/// A CLI running in a session of its own, with the lines of its stdout and stderr (escape
/// sequences stripped, trimmed) arriving on [`Self::lines`]. A cancel ([`Self::stop`]) kills the
/// whole session, and so does dropping it, as a flow's end by a timeout or the daemon's exit does:
/// some helpers of an agent CLI move into process groups of their own, where a signal to its group
/// would miss them. A CLI that exits by itself is only reaped, its session left as it is: the
/// browser a login opens can be in it, started directly by `xdg-open` on a Linux desktop.
pub(crate) struct Process {
    pub(crate) child: Child,
    /// The CLI as the platform kills and sweeps it, while `child` is not reaped.
    handle: ChildHandle,
    /// Names the CLI in the log of a session sweep: the agent's name.
    label: &'static str,
    stdin: Option<ChildStdin>,
    pub(crate) lines: mpsc::Receiver<Line>,
}

impl Process {
    pub(crate) fn spawn(
        mut command: std::process::Command,
        label: &'static str,
        pipe_stdin: bool,
    ) -> std::io::Result<Self> {
        proc::lead_session(&mut command);
        let mut command = tokio::process::Command::from(command);
        command
            .stdin(if pipe_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let taken_on = child.id().map_or_else(
            || Err(std::io::ErrorKind::NotFound.into()),
            |pid| ChildHandle::started(ProcessId(pid), Reach::Session, None),
        );
        let handle = match taken_on {
            Ok(handle) => handle,
            Err(err) => {
                let _ = child.start_kill();
                return Err(err);
            }
        };
        let (tx, lines) = line_channel();
        if let Some(stdout) = child.stdout.take() {
            forward_lines(stdout, false, tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            forward_lines(stderr, true, tx);
        }
        Ok(Self {
            stdin: child.stdin.take(),
            child,
            handle,
            label,
            lines,
        })
    }

    /// The CLI as the platform kills and sweeps it, for a test to look at.
    #[cfg(test)]
    pub(crate) fn handle(&self) -> &ChildHandle {
        &self.handle
    }

    /// Writes `request` as a line to the CLI's stdin.
    pub(crate) async fn send(&mut self, request: &Value) -> std::io::Result<()> {
        let stdin = self.stdin.as_mut().ok_or(std::io::ErrorKind::BrokenPipe)?;
        stdin.write_all(format!("{request}\n").as_bytes()).await?;
        stdin.flush().await
    }

    /// Kills the CLI's whole session and waits for the CLI to be gone, so that what it held
    /// (Codex's login port) is free when this returns. The session is swept once the CLI has
    /// exited, found out without reaping it (`WNOWAIT`), while its zombie keeps the session id
    /// from passing to another process.
    pub(crate) async fn stop(&mut self) {
        // `id` is `None` once the child has been reaped, when its pid may be someone else's.
        if self.child.id().is_some() {
            let child = &self.handle;
            child.kill();
            loop {
                match child.exited_unreaped() {
                    Some(false) => tokio::time::sleep(EXIT_POLL).await,
                    Some(true) => {
                        child.kill_leftovers(&[]).log(child.pid(), self.label);
                        break;
                    }
                    // `waitid` cannot tell, which it also says of a child already reaped, whose
                    // pid may be someone else's: no sweep, and `wait` decides.
                    None => break,
                }
            }
        }
        let _ = self.child.wait().await;
    }

    /// Takes what the CLI printed last into `last_words`, a moment's worth: the line that says
    /// why it failed comes just before it exits.
    pub(crate) async fn last_words(&mut self, last_words: &mut String) {
        self.drain(|line| {
            if let Some(words) = line.into_words() {
                *last_words = words;
            }
        })
        .await;
    }

    /// Hands every line the CLI has still to say to `each`, a moment's worth after the last: a
    /// helper that kept the pipes open must not hold the login for good.
    pub(crate) async fn drain(&mut self, mut each: impl FnMut(Line)) {
        while let Ok(Some(line)) = tokio::time::timeout(LAST_WORDS, self.lines.recv()).await {
            each(line);
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // Swept without waiting for the CLI to exit, which a drop cannot do, so a process it
        // starts while the kill is landing is missed.
        if self.child.id().is_some() {
            self.handle.kill();
            self.handle
                .kill_leftovers(&[])
                .log(self.handle.pid(), self.label);
        }
    }
}

/// The most of a line of a CLI's output that is kept; the rest of a longer line is dropped.
const LINE_LIMIT: usize = 16 * 1024;

/// How many lines wait for a flow that is not reading them. Past it the newest are dropped, so a
/// CLI that floods its output neither grows the daemon's memory nor stalls on a full pipe.
const LINE_BACKLOG: usize = 256;

/// One line of a CLI's output.
pub(crate) struct Line {
    pub(crate) text: String,
    /// Whether the CLI wrote it to its stderr.
    pub(crate) stderr: bool,
    /// Whether the CLI's line was longer than [`LINE_LIMIT`] and `text` is only its start.
    cut: bool,
}

impl Line {
    /// What the line says, for a CLI's last words: `None` for an empty line and for a cut one,
    /// whose end, the part that would tell something, is gone.
    pub(crate) fn into_words(self) -> Option<String> {
        (!self.text.is_empty() && !self.cut).then_some(self.text)
    }
}

/// The channel a CLI's lines wait in, bounded by [`LINE_BACKLOG`].
fn line_channel() -> (mpsc::Sender<Line>, mpsc::Receiver<Line>) {
    mpsc::channel(LINE_BACKLOG)
}

/// Reads the next line of `reader` into `line`, keeping at most [`LINE_LIMIT`] bytes of it.
/// `None` at the end of the pipe, else whether the line was cut.
async fn read_line_capped(
    reader: &mut BufReader<impl tokio::io::AsyncRead + Unpin>,
    line: &mut Vec<u8>,
) -> std::io::Result<Option<bool>> {
    line.clear();
    let mut any = false;
    let mut cut = false;
    loop {
        let buffered = reader.fill_buf().await?;
        if buffered.is_empty() {
            return Ok(any.then_some(cut));
        }
        any = true;
        let (used, complete) = match buffered.iter().position(|byte| *byte == b'\n') {
            Some(end) => (end + 1, true),
            None => (buffered.len(), false),
        };
        let room = LINE_LIMIT.saturating_sub(line.len());
        let content = if complete { used - 1 } else { used };
        cut |= content > room;
        line.extend_from_slice(&buffered[..used.min(room)]);
        reader.consume(used);
        if complete {
            return Ok(Some(cut));
        }
    }
}

fn forward_lines(
    pipe: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    stderr: bool,
    tx: mpsc::Sender<Line>,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(pipe);
        let mut bytes = Vec::new();
        // Read as bytes and decoded lossily: a line that is not UTF-8 must not stop the pipe from
        // being read, as a full one stalls the CLI.
        while let Ok(Some(cut)) = read_line_capped(&mut reader, &mut bytes).await {
            let line = String::from_utf8_lossy(&bytes);
            let line = Line {
                text: strip_escapes(&line).trim().to_string(),
                stderr,
                cut,
            };
            match tx.try_send(line) {
                Err(mpsc::error::TrySendError::Closed(_)) => return,
                Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::helper_in_its_own_group;
    use crate::test_support::{
        assert_stops_running, is_running, read_pid_file, KillOnDrop, ScratchFile, PATIENCE,
    };

    /// What `forward_lines` hands over for `input` once its sender is gone, as (text, cut).
    async fn forwarded(input: Vec<u8>) -> Vec<(String, bool)> {
        let (tx, mut lines) = line_channel();
        forward_lines(std::io::Cursor::new(input), false, tx);
        let mut got = Vec::new();
        // Nothing receives until the reader has ended, so only the backlog is kept for it.
        while let Some(line) = lines.recv().await {
            got.push((line.text, line.cut));
        }
        got
    }

    #[tokio::test]
    async fn a_line_that_is_not_utf8_does_not_stop_the_lines_after_it() {
        let got = forwarded(b"bad \xff line\n\x1b[1mnext\x1b[0m\nlast".to_vec()).await;
        let texts: Vec<&str> = got.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(texts, ["bad \u{fffd} line", "next", "last"]);
    }

    #[tokio::test]
    async fn the_caps_on_a_cli_s_output_hold() {
        let kept = |text: &str| (text.to_string(), false);
        let mut flood = Vec::new();
        for n in 0..LINE_BACKLOG + 50 {
            flood.extend_from_slice(format!("{n}\n").as_bytes());
        }
        let mut exact = "y".repeat(LINE_LIMIT).into_bytes();
        exact.extend_from_slice(b"\nnext\n");
        let cases = [
            (
                "a line past the limit is cut and the next one still arrives",
                [&b"x".repeat(LINE_LIMIT + 7)[..], b"\nnext\n"].concat(),
                vec![("x".repeat(LINE_LIMIT), true), kept("next")],
            ),
            (
                "a line of exactly the limit is whole",
                exact,
                vec![("y".repeat(LINE_LIMIT), false), kept("next")],
            ),
            (
                "lines past the backlog are dropped, the first ones kept",
                flood,
                (0..LINE_BACKLOG).map(|n| kept(&n.to_string())).collect(),
            ),
        ];
        for (name, input, expected) in cases {
            assert_eq!(forwarded(input).await, expected, "{name}");
        }
    }

    #[test]
    fn a_cut_or_empty_line_is_not_a_cli_s_last_words() {
        let line = |text: &str, cut| Line {
            text: text.to_string(),
            stderr: false,
            cut,
        };
        assert_eq!(
            line("failed", false).into_words().as_deref(),
            Some("failed")
        );
        assert_eq!(line("failed", true).into_words(), None);
        assert_eq!(line("", false).into_words(), None);
    }

    /// Starts a stand-in login CLI that leaves a helper in a process group of its own and then
    /// exits, or keeps running unless `exits`, and returns it with the helper's pid once the
    /// helper is running.
    async fn stand_in_with_a_helper(exits: bool) -> (Process, i32) {
        let pid_file = ScratchFile::new("login-helper", "pid");
        let command = helper_leaving_cli(&pid_file, exits);
        let process = Process::spawn(command, "stand-in", false).expect("the stand-in starts");
        let deadline = tokio::time::Instant::now() + PATIENCE;
        loop {
            if let Some(pid) = read_pid_file(&pid_file) {
                return (process, pid);
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the stand-in never recorded its helper"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// The stand-in of [`stand_in_with_a_helper`], writing its helper's pid to `pid_file`.
    #[cfg(unix)]
    fn helper_leaving_cli(pid_file: &std::path::Path, exits: bool) -> std::process::Command {
        let then = if exits {
            "exit 0"
        } else {
            "exec /bin/sleep 300"
        };
        let mut command = std::process::Command::new(crate::test_support::bash());
        command.args([
            "-c",
            &format!("{}; {then}", helper_in_its_own_group(pid_file)),
        ]);
        command
    }

    /// The stand-in of [`stand_in_with_a_helper`] on Windows, a fixture process with its helper on
    /// its console. It writes the helper's pid to `pid_file` once the helper has written it beside
    /// it, which the helper does once it runs, and so is on the console by then.
    #[cfg(windows)]
    fn helper_leaving_cli(pid_file: &std::path::Path, exits: bool) -> std::process::Command {
        use crate::test_support::fixture::Steps;
        let running = pid_file.with_extension("helper");
        let steps = Steps::new()
            .spawn_on_console("helper", Steps::new().write_pid(&running).sleep(300))
            .wait_for(&running)
            .write_child_pid(pid_file);
        let steps = if exits { steps } else { steps.sleep(300) };
        steps.program("login-cli").command()
    }

    #[tokio::test]
    async fn a_login_cli_stopped_or_dropped_leaves_no_helper_behind() {
        for stopped in [true, false] {
            let (mut process, helper) = stand_in_with_a_helper(false).await;
            let mut guard = KillOnDrop(vec![helper]);
            if stopped {
                process.stop().await;
            } else {
                drop(process);
            }
            assert_stops_running(helper);
            guard.0.clear();
        }
    }

    #[tokio::test]
    async fn dropping_a_login_cli_that_was_reaped_leaves_its_helpers_running() {
        let (mut process, helper) = stand_in_with_a_helper(true).await;
        let _guard = KillOnDrop(vec![helper]);
        let status = process.child.wait().await.expect("its status");
        assert!(status.success());
        drop(process);
        // Long enough for a kill sent on the drop to have landed.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            is_running(helper),
            "the helper of a CLI that exited was killed"
        );
    }
}
