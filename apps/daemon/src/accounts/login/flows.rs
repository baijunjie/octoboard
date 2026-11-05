//! What drives an agent's CLI to sign an account in, whichever the agent: the [`Flow`] to run, the
//! [`Process`] it runs in and what it reports through [`Progress`]. How each CLI is driven is the
//! provider's own ([`LoginFlow`], in `crate::adapter::<provider>::login`), as measured in
//! `docs/agent-cli-reference.md` ("Signing in a config directory without a terminal"). All run
//! with no terminal and nothing written to their stdin beyond what the provider writes.
//!
//! Dropping a flow's future kills the CLI's whole session (see [`Process`]), which is how a timeout
//! and the daemon's exit end one, and removes the flow's scratch directory.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{mpsc, Notify};

use super::Key;
use crate::adapter::{self, Unstartable};
use crate::platform::proc::{self, ChildHandle, ProcessId, Reach};
use crate::platform::shell::ResolvedBinary;
use crate::protocol::{login_failure_code as code, AgentId, LoginFailure, LoginStatus};
use crate::providers::ProviderCode;
use crate::state::AppState;
use crate::terminal::plain_text::strip_escapes;

/// How a login ended.
pub(crate) enum Outcome {
    Succeeded,
    Failed(LoginFailure),
    Cancelled,
    TimedOut,
}

/// How long a CLI's last words are waited for after it exits.
const LAST_WORDS: Duration = Duration::from_millis(300);

/// How long a CLI that could not be written to is given to exit, so that its exit, and what it
/// said, is what the login reports.
const WRITE_FAILURE_GRACE: Duration = Duration::from_secs(1);

/// How often a CLI that has been killed is looked at for having exited, which is found out
/// without reaping it so that what it left in its session can be killed first (see
/// [`Process::stop`]).
const EXIT_POLL: Duration = Duration::from_millis(20);

/// Reports a login's progress to the clients.
pub(crate) struct Progress {
    state: Arc<AppState>,
    key: Key,
}

impl Progress {
    pub(super) fn new(state: Arc<AppState>, key: Key) -> Self {
        Self { state, key }
    }

    /// The user has something to do: authorize on `url`, confirming `device_code` there if there
    /// is one. `opened_by_agent` is the CLI having opened the page itself.
    pub(crate) fn waiting(
        &self,
        url: Option<String>,
        device_code: Option<String>,
        opened_by_agent: bool,
    ) {
        self.state
            .account_logins
            .update(&self.state, &self.key, |login| {
                login.status = LoginStatus::Waiting;
                login.authorization_url = url.or(login.authorization_url.take());
                login.device_code = device_code.or(login.device_code.take());
                login.opened_by_agent = opened_by_agent;
            });
    }
}

/// A sign-in counted under its account's directory, which refuses a change of the account's
/// sharing; every client is told when it starts and when it ends.
struct SigningIn {
    state: Arc<AppState>,
    run: Option<crate::accounts::shared_config::Run>,
}

impl SigningIn {
    fn new(state: Arc<AppState>, run: crate::accounts::shared_config::Run) -> Self {
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

/// How one provider's CLI is driven to sign an account in.
pub(crate) trait LoginFlow: Send + Sync {
    /// Runs the CLI's sign-in for the account `flow` describes (binary, shell environment,
    /// directories, scratch), reporting what the user has to do through `progress`, until it ends
    /// or `cancel` is notified. A boxed future because the crate has no `async-trait`.
    fn run<'a>(
        &'a self,
        flow: &'a Flow,
        progress: &'a Progress,
        cancel: &'a Notify,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;

    /// The code a sign-in is refused with while another account of this provider is being signed
    /// in, for a provider whose sign-in only one login at a time can use (one that listens on a
    /// fixed local port); `None`, the default, lets its logins run side by side.
    fn busy_code(&self) -> Option<ProviderCode> {
        None
    }
}

/// One agent's login, ready to run.
pub(crate) struct Flow {
    pub(crate) agent: AgentId,
    /// How the agent's sign-in is driven.
    login: &'static dyn LoginFlow,
    /// Where the agent's binary resolved, once, so that every command of the flow runs the same.
    binary: ResolvedBinary,
    pub(crate) shell_env: HashMap<String, String>,
    /// The account's directory, exported to the CLI; `None` for the default account.
    pub(crate) config_dir: Option<PathBuf>,
    /// Where the credential ends up, if known: the pinned directory or the agent's own default.
    pub(crate) resolved_dir: Option<PathBuf>,
    /// This login's own scratch directory, removed when the flow is dropped.
    scratch: Scratch,
}

/// A scratch directory removed on drop, so that a flow dropped by a timeout or the daemon's exit
/// leaves nothing behind either.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Flow {
    /// `Err` is why the agent's binary cannot start its CLI in `shell_env`.
    pub(super) fn new(
        agent: AgentId,
        login: &'static dyn LoginFlow,
        shell_env: HashMap<String, String>,
        config_dir: Option<PathBuf>,
        resolved_dir: Option<PathBuf>,
        scratch: PathBuf,
    ) -> Result<Self, Unstartable> {
        let binary = adapter::resolve_account_binary(agent, &shell_env)?;
        Ok(Self {
            agent,
            login,
            binary,
            shell_env,
            config_dir,
            resolved_dir,
            scratch: Scratch(scratch),
        })
    }

    pub(super) async fn run(self, progress: &Progress, cancel: &Notify) -> Outcome {
        // The account's directory must not change sharing under the CLI while it signs in. A login
        // waiting for a change being carried out there is still cancelled at once.
        let dir = self.config_dir.clone();
        let waiting = tokio::task::spawn_blocking(move || {
            crate::accounts::shared_config::running_long(dir.as_deref())
        });
        let running = tokio::select! {
            running = waiting => running.ok().flatten(),
            () = cancel.notified() => return Outcome::Cancelled,
        };
        let _signing_in = running.map(|run| SigningIn::new(progress.state.clone(), run));
        self.login.run(&self, progress, cancel).await
    }

    /// A flow of `agent` for a test, its binary not resolved.
    #[cfg(test)]
    pub(crate) fn unresolved(agent: AgentId, scratch: PathBuf) -> Self {
        Self {
            agent,
            login: adapter::provider(agent)
                .accounts()
                .and_then(|ops| ops.login())
                .expect("the agent has a sign-in flow"),
            binary: ResolvedBinary::default(),
            shell_env: HashMap::new(),
            config_dir: None,
            resolved_dir: None,
            scratch: Scratch(scratch),
        }
    }

    /// This login's own scratch directory, created by whatever needs it.
    pub(crate) fn scratch_dir(&self) -> &Path {
        &self.scratch.0
    }

    pub(crate) fn command(&self, args: &[&str]) -> std::process::Command {
        adapter::account_command_of(
            &self.binary,
            self.agent,
            &self.shell_env,
            self.config_dir.as_deref(),
            args,
        )
    }

    pub(crate) fn failed(&self, detail: &str) -> Outcome {
        Outcome::Failed(LoginFailure::new(
            code::FAILED,
            format!("{} could not sign in: {detail}", self.agent.label()),
            &[("agent", self.agent.label()), ("detail", detail)],
        ))
    }

    pub(crate) fn not_confirmed(&self) -> Outcome {
        Outcome::Failed(LoginFailure::new(
            code::NOT_CONFIRMED,
            format!(
                "{} finished, but is still not signed in for this account",
                self.agent.label()
            ),
            &[("agent", self.agent.label())],
        ))
    }

    /// How a CLI that ended without signing in is reported: its last words, else its exit status.
    pub(crate) fn ended(&self, status: std::process::ExitStatus, last_words: &str) -> Outcome {
        if last_words.is_empty() {
            self.failed(&format!("it ended with {status}"))
        } else {
            self.failed(last_words)
        }
    }

    /// Writes `request` to the CLI, or says how the login ends when that fails. A CLI that exits
    /// at once, on a bad configuration say, can close its stdin before it is written to, and what
    /// it said is why the login failed, not the broken pipe: a failed write is reported as the
    /// CLI's exit, with `last_words` (what it has said so far) brought up to date. Only a CLI still
    /// running after [`WRITE_FAILURE_GRACE`] is reported as not taking the write.
    pub(crate) async fn send(
        &self,
        process: &mut Process,
        request: &Value,
        last_words: &mut String,
    ) -> Result<(), Outcome> {
        let Err(err) = process.send(request).await else {
            return Ok(());
        };
        match tokio::time::timeout(WRITE_FAILURE_GRACE, process.child.wait()).await {
            Ok(Ok(status)) => {
                process.last_words(last_words).await;
                Err(self.ended(status, last_words))
            }
            _ => Err(self.failed(&format!("it could not be written to: {err}"))),
        }
    }
}

/// The last non-empty line of the file at `path`; empty while it is missing.
pub(crate) fn last_line_of(path: &Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            text.lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default()
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
            forward_lines(stdout, tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            forward_lines(stderr, tx);
        }
        Ok(Self {
            stdin: child.stdin.take(),
            child,
            handle,
            label,
            lines,
        })
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
        while let Ok(Some(line)) = tokio::time::timeout(LAST_WORDS, self.lines.recv()).await {
            if let Some(words) = line.into_words() {
                *last_words = words;
            }
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

fn forward_lines(pipe: impl tokio::io::AsyncRead + Unpin + Send + 'static, tx: mpsc::Sender<Line>) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(pipe);
        let mut bytes = Vec::new();
        // Read as bytes and decoded lossily: a line that is not UTF-8 must not stop the pipe from
        // being read, as a full one stalls the CLI.
        while let Ok(Some(cut)) = read_line_capped(&mut reader, &mut bytes).await {
            let line = String::from_utf8_lossy(&bytes);
            let line = Line {
                text: strip_escapes(&line).trim().to_string(),
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
        assert_stops_running, is_running, read_pid_file, KillOnDrop, ScratchDir, ScratchFile,
        PATIENCE,
    };
    use serde_json::json;

    /// What `forward_lines` hands over for `input` once its sender is gone, as (text, cut).
    async fn forwarded(input: Vec<u8>) -> Vec<(String, bool)> {
        let (tx, mut lines) = line_channel();
        forward_lines(std::io::Cursor::new(input), tx);
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

    #[tokio::test]
    async fn a_write_to_a_cli_that_has_exited_reports_what_it_said_last() {
        let scratch = ScratchDir::new("login-write-after-exit");
        let flow = Flow::unresolved(AgentId::CODEX, scratch.to_path_buf());
        let mut command = std::process::Command::new(crate::test_support::bash());
        command.args(["-c", "echo 'Error: bad configuration' >&2; exit 1"]);
        let mut process = Process::spawn(command, "stand-in", true).expect("the stand-in starts");
        // Exited, found out without reaping it, so the write below meets a closed pipe.
        let child = &process.handle;
        let deadline = tokio::time::Instant::now() + PATIENCE;
        while child.exited_unreaped() != Some(true) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the stand-in never exited"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // The pipe can still have a reader for a moment: a process forked by another thread
        // while this one was spawning the stand-in inherited both ends, and holds them until it
        // execs. A write that lands meanwhile is retried, the stand-in still unreaped, until one
        // meets the closed pipe.
        let mut last_words = String::new();
        let outcome = loop {
            match flow
                .send(&mut process, &json!({"id": 1}), &mut last_words)
                .await
            {
                Err(outcome) => break outcome,
                Ok(()) => {
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "something kept reading the stand-in's stdin"
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        };
        let Outcome::Failed(failure) = outcome else {
            panic!("the login did not fail");
        };
        assert_eq!(
            failure.params.get("detail").map(String::as_str),
            Some("Error: bad configuration")
        );
    }
}
