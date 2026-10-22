//! How each agent's CLI is driven to sign an account in, as measured in
//! `docs/agent-cli-reference.md` ("Signing in a config directory without a terminal"). All three
//! run with no terminal and nothing written to their stdin beyond what is said below:
//!
//! - **Claude Code** is `claude auth login` with `BROWSER` pointed at a script that appends the
//!   URL it is given to a file the flow watches. That URL is the one whose loopback callback
//!   finishes the login with nothing pasted. Done is exit 0, confirmed by `claude auth status`.
//! - **Codex** is `codex app-server`: `account/login/start` answers with the authorization URL,
//!   `account/login/completed` says how it went, and `account/read` confirms. The config directory
//!   is created first, as Codex refuses a missing one; for the default account that is the home
//!   the agent resolved.
//! - **Grok Build** is `grok login --device-auth`, which prints the device URL and code on stderr
//!   and opens the browser itself, which cannot be stopped. Done is exit 0 with `auth.json` in its
//!   home.
//!
//! Dropping a flow's future kills the CLI's whole session (see [`Process`]), which is how a timeout
//! and the daemon's exit end one, and removes the flow's scratch directory.

use std::collections::HashMap;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{mpsc, Notify};

use super::Key;
use crate::account_quotas::CODEX_APP_SERVER_ARGS;
use crate::adapter;
use crate::process_session;
use crate::protocol::{login_failure_code as code, Agent, LoginFailure, LoginStatus};
use crate::state::AppState;

/// How a login ended.
pub(super) enum Outcome {
    Succeeded,
    Failed(LoginFailure),
    Cancelled,
    TimedOut,
}

/// How often the file Claude Code's `BROWSER` script writes is looked at.
const URL_POLL: Duration = Duration::from_millis(100);

/// How long a CLI's last words are waited for after it exits.
const LAST_WORDS: Duration = Duration::from_millis(300);

/// How long a CLI that could not be written to is given to exit, so that its exit, and what it
/// said, is what the login reports.
const WRITE_FAILURE_GRACE: Duration = Duration::from_secs(1);

/// How long Codex's app server is given to answer a cancel before it is killed anyway.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

/// How long `claude auth status` may take.
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);

/// How often a CLI that has been killed is looked at for having exited, which is found out
/// without reaping it so that what it left in its session can be killed first (see
/// [`Process::stop`]).
const EXIT_POLL: Duration = Duration::from_millis(20);

/// Reports a login's progress to the clients.
pub(super) struct Progress {
    state: Arc<AppState>,
    key: Key,
}

impl Progress {
    pub(super) fn new(state: Arc<AppState>, key: Key) -> Self {
        Self { state, key }
    }

    /// The user has something to do: authorize on `url`, confirming `device_code` there if there
    /// is one. `opened_by_agent` is the CLI having opened the page itself.
    fn waiting(&self, url: Option<String>, device_code: Option<String>, opened_by_agent: bool) {
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
    run: Option<crate::shared_config::Run>,
}

impl SigningIn {
    fn new(state: Arc<AppState>, run: crate::shared_config::Run) -> Self {
        crate::shared_config::publish(&state);
        Self {
            state,
            run: Some(run),
        }
    }
}

impl Drop for SigningIn {
    fn drop(&mut self) {
        drop(self.run.take());
        crate::shared_config::publish(&self.state);
    }
}

/// One agent's login, ready to run.
pub(super) struct Flow {
    agent: Agent,
    /// Where the agent's binary resolved, once, so that every command of the flow runs the same.
    binary: String,
    shell_env: HashMap<String, String>,
    /// The account's directory, exported to the CLI; `None` for the default account.
    config_dir: Option<PathBuf>,
    /// Where the credential ends up, if known: the pinned directory or the agent's own default.
    resolved_dir: Option<PathBuf>,
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
    /// `Err` carries the binary's name when it does not resolve in `shell_env`.
    pub(super) fn new(
        agent: Agent,
        shell_env: HashMap<String, String>,
        config_dir: Option<PathBuf>,
        resolved_dir: Option<PathBuf>,
        scratch: PathBuf,
    ) -> Result<Self, &'static str> {
        let binary = adapter::resolve_account_binary(agent, &shell_env)?;
        Ok(Self {
            agent,
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
        let waiting =
            tokio::task::spawn_blocking(move || crate::shared_config::running_long(dir.as_deref()));
        let running = tokio::select! {
            running = waiting => running.ok().flatten(),
            () = cancel.notified() => return Outcome::Cancelled,
        };
        let _signing_in = running.map(|run| SigningIn::new(progress.state.clone(), run));
        match self.agent {
            Agent::Claude => self.claude(progress, cancel).await,
            Agent::Codex => self.codex(progress, cancel).await,
            Agent::Grok => self.grok(progress, cancel).await,
        }
    }

    fn command(&self, args: &[&str]) -> std::process::Command {
        adapter::account_command_of(
            &self.binary,
            self.agent,
            &self.shell_env,
            self.config_dir.as_deref(),
            args,
        )
    }

    fn failed(&self, detail: &str) -> Outcome {
        Outcome::Failed(LoginFailure::new(
            code::FAILED,
            format!("{} could not sign in: {detail}", self.agent.label()),
            &[("agent", self.agent.label()), ("detail", detail)],
        ))
    }

    fn not_confirmed(&self) -> Outcome {
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
    fn ended(&self, status: std::process::ExitStatus, last_words: &str) -> Outcome {
        if last_words.is_empty() {
            self.failed(&format!("it ended with {status}"))
        } else {
            self.failed(last_words)
        }
    }

    async fn claude(&self, progress: &Progress, cancel: &Notify) -> Outcome {
        let (script, urls) = match self.write_browser_script() {
            Ok(written) => written,
            Err(err) => return self.failed(&format!("preparing the sign-in failed: {err}")),
        };
        let mut command = self.command(&["auth", "login"]);
        command.env("BROWSER", script);
        let mut process = match Process::spawn(command, self.agent.label(), false) {
            Ok(process) => process,
            Err(err) => return self.failed(&format!("it could not be started: {err}")),
        };

        let mut reported = String::new();
        let mut last_words = String::new();
        let mut poll = tokio::time::interval(URL_POLL);
        let status = loop {
            tokio::select! {
                _ = poll.tick() => {
                    let url = last_line_of(&urls);
                    if !url.is_empty() && url != reported {
                        progress.waiting(Some(url.clone()), None, false);
                        reported = url;
                    }
                }
                Some(line) = process.lines.recv() => {
                    if !line.is_empty() {
                        last_words = line;
                    }
                }
                status = process.child.wait() => break status,
                () = cancel.notified() => {
                    process.stop().await;
                    return Outcome::Cancelled;
                }
            }
        };
        let status = match status {
            Ok(status) => status,
            Err(err) => return self.failed(&err.to_string()),
        };
        process.last_words(&mut last_words).await;
        if !status.success() {
            return self.ended(status, &last_words);
        }
        // The CLI has gone, but a cancel is still honoured while its answer is waited for.
        let signed_in = tokio::select! {
            signed_in = self.claude_signed_in() => signed_in,
            () = cancel.notified() => return Outcome::Cancelled,
        };
        if signed_in {
            Outcome::Succeeded
        } else {
            self.not_confirmed()
        }
    }

    /// Writes the `BROWSER` script, which appends its argument to the returned file, into the
    /// scratch directory. A script because `BROWSER` is run as `$BROWSER <url>`.
    fn write_browser_script(&self) -> std::io::Result<(PathBuf, PathBuf)> {
        use std::os::unix::fs::PermissionsExt;

        // Private to the user, as the script is run by whatever the CLI hands the URL to.
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.scratch.0)?;
        let (script, urls) = (
            self.scratch.0.join("browser.sh"),
            self.scratch.0.join("urls"),
        );
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$1\" >> {}\n",
                shell_quote(&urls)
            ),
        )?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        Ok((script, urls))
    }

    /// Whether `claude auth status` says the account is signed in. It exits 1 when it is not, with
    /// the answer on stdout either way, so the exit status is not looked at.
    async fn claude_signed_in(&self) -> bool {
        let mut command = self.command(&["auth", "status"]);
        let output = tokio::task::spawn_blocking(move || {
            crate::subprocess::run_agent_with_timeout(&mut command, STATUS_TIMEOUT)
        })
        .await;
        let Ok(Ok(output)) = output else {
            return false;
        };
        serde_json::from_slice::<Value>(&output.stdout)
            .ok()
            .and_then(|status| status.get("loggedIn").and_then(Value::as_bool))
            .unwrap_or(false)
    }

    async fn grok(&self, progress: &Progress, cancel: &Notify) -> Outcome {
        let mut process = match Process::spawn(
            self.command(&["login", "--device-auth"]),
            self.agent.label(),
            false,
        ) {
            Ok(process) => process,
            Err(err) => return self.failed(&format!("it could not be started: {err}")),
        };
        let (mut url, mut device_code) = (None, None);
        let mut last_words = String::new();
        let status = loop {
            tokio::select! {
                Some(line) = process.lines.recv() => {
                    let mut found = false;
                    if url.is_none() {
                        if let Some(page) = url_in(&line) {
                            device_code = device_code.take().or_else(|| user_code_in(&page));
                            url = Some(page);
                            found = true;
                        }
                    }
                    if device_code.is_none() && looks_like_device_code(&line) {
                        device_code = Some(line.clone());
                        found = true;
                    }
                    if found {
                        progress.waiting(url.clone(), device_code.clone(), true);
                    }
                    if !line.is_empty() {
                        last_words = line;
                    }
                }
                status = process.child.wait() => break status,
                () = cancel.notified() => {
                    process.stop().await;
                    return Outcome::Cancelled;
                }
            }
        };
        let status = match status {
            Ok(status) => status,
            Err(err) => return self.failed(&err.to_string()),
        };
        process.last_words(&mut last_words).await;
        if !status.success() {
            return self.ended(status, &last_words);
        }
        match &self.resolved_dir {
            Some(dir) if !dir.join("auth.json").is_file() => self.not_confirmed(),
            _ => Outcome::Succeeded,
        }
    }

    /// Writes `request` to the CLI, or says how the login ends when that fails. A CLI that exits
    /// at once, on a bad configuration say, can close its stdin before it is written to, and what
    /// it said is why the login failed, not the broken pipe: a failed write is reported as the
    /// CLI's exit, with `last_words` (what it has said so far) brought up to date. Only a CLI still
    /// running after [`WRITE_FAILURE_GRACE`] is reported as not taking the write.
    async fn send(
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

    async fn codex(&self, progress: &Progress, cancel: &Notify) -> Outcome {
        // Codex refuses a home that does not exist, the default account's included.
        if let Some(dir) = self.config_dir.as_ref().or(self.resolved_dir.as_ref()) {
            if let Err(err) = std::fs::create_dir_all(dir) {
                return self.failed(&format!("creating {} failed: {err}", dir.display()));
            }
        }
        let mut process = match Process::spawn(
            self.command(CODEX_APP_SERVER_ARGS),
            self.agent.label(),
            true,
        ) {
            Ok(process) => process,
            Err(err) => return self.failed(&format!("it could not be started: {err}")),
        };
        let requests = [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"clientInfo": {"name": "octoboard",
                    "version": env!("CARGO_PKG_VERSION")}}}),
            json!({"jsonrpc": "2.0", "method": "initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "account/login/start",
                "params": {"type": "chatgpt"}}),
        ];
        let mut last_words = String::new();
        for request in &requests {
            if let Err(outcome) = self.send(&mut process, request, &mut last_words).await {
                return outcome;
            }
        }

        let mut login_id: Option<String> = None;
        loop {
            tokio::select! {
                Some(line) = process.lines.recv() => {
                    let Ok(message) = serde_json::from_str::<Value>(&line) else {
                        if !line.is_empty() {
                            last_words = line;
                        }
                        continue;
                    };
                    match codex_event(&message, login_id.as_deref()) {
                        CodexEvent::Started { login, url } => {
                            login_id = Some(login);
                            progress.waiting(Some(url), None, false);
                        }
                        CodexEvent::Completed => {
                            let read = json!({"jsonrpc": "2.0", "id": 3, "method": "account/read",
                                "params": {}});
                            if let Err(outcome) =
                                self.send(&mut process, &read, &mut last_words).await
                            {
                                return outcome;
                            }
                        }
                        CodexEvent::Confirmed(true) => return Outcome::Succeeded,
                        CodexEvent::Confirmed(false) => return self.not_confirmed(),
                        CodexEvent::Rejected(detail) => return self.failed(&detail),
                        CodexEvent::GaveUp(detail) if is_timeout(&detail) => {
                            return Outcome::TimedOut;
                        }
                        CodexEvent::GaveUp(detail) => return self.failed(&detail),
                        CodexEvent::Other => {}
                    }
                }
                status = process.child.wait() => {
                    return match status {
                        Ok(status) => {
                            process.last_words(&mut last_words).await;
                            self.ended(status, &last_words)
                        }
                        Err(err) => self.failed(&err.to_string()),
                    };
                }
                () = cancel.notified() => {
                    if let Some(login) = &login_id {
                        let cancel = json!({"jsonrpc": "2.0", "id": 4,
                            "method": "account/login/cancel", "params": {"loginId": login}});
                        // Best effort: its listener goes with the process either way, but this
                        // lets it close the flow itself first.
                        if process.send(&cancel).await.is_ok() {
                            let completed = process.until_completed();
                            let _ = tokio::time::timeout(CANCEL_GRACE, completed).await;
                        }
                    }
                    process.stop().await;
                    return Outcome::Cancelled;
                }
            }
        }
    }
}

/// Whether Codex's account of why a login did not complete says it ran out of time.
fn is_timeout(detail: &str) -> bool {
    detail.to_lowercase().contains("timed out")
}

/// What a line of Codex's app server means to the login.
enum CodexEvent {
    /// The reply to `account/login/start`.
    Started {
        login: String,
        url: String,
    },
    /// `account/login/completed` with `success: true`.
    Completed,
    /// The reply to `account/read`: whether an account is there.
    Confirmed(bool),
    /// The reply to `account/login/start` was an error.
    Rejected(String),
    /// `account/login/completed` with `success: false`, and what it said.
    GaveUp(String),
    Other,
}

fn codex_event(message: &Value, login: Option<&str>) -> CodexEvent {
    // A reply carries the id of its request and no method; a message with a method is the server's
    // own, whatever id it numbers itself by.
    let id = message
        .get("method")
        .is_none()
        .then(|| message.get("id").and_then(Value::as_i64))
        .flatten();
    if id == Some(2) {
        if let Some(error) = message.get("error") {
            return CodexEvent::Rejected(rpc_message(error));
        }
        let result = message.get("result");
        let field = |name: &str| {
            result
                .and_then(|result| result.get(name))
                .and_then(Value::as_str)
        };
        return match (field("loginId"), field("authUrl")) {
            (Some(login), Some(url)) => CodexEvent::Started {
                login: login.to_string(),
                url: url.to_string(),
            },
            _ => CodexEvent::Rejected("the login start was answered without a page".to_string()),
        };
    }
    if id == Some(3) {
        let signed_in = message
            .pointer("/result/account")
            .is_some_and(|account| !account.is_null());
        return CodexEvent::Confirmed(signed_in);
    }
    if message.get("method").and_then(Value::as_str) == Some("account/login/completed") {
        let params = message.get("params");
        let named = params
            .and_then(|params| params.get("loginId"))
            .and_then(Value::as_str);
        if login.is_some() && named.is_some() && named != login {
            return CodexEvent::Other;
        }
        if params.and_then(|params| params.get("success")) == Some(&Value::Bool(true)) {
            return CodexEvent::Completed;
        }
        let detail = params
            .and_then(|params| params.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("the login did not complete");
        return CodexEvent::GaveUp(detail.to_string());
    }
    CodexEvent::Other
}

fn rpc_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("the request was refused")
        .to_string()
}

/// The last non-empty line of the file at `path`; empty while it is missing.
fn last_line_of(path: &Path) -> String {
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

/// `path` as one word for `sh`.
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// The first `https://` word of `line`.
fn url_in(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|word| word.starts_with("https://"))
        .map(str::to_string)
}

/// The `user_code` of a device-flow page's address.
fn user_code_in(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("user_code="))
        .filter(|code| !code.is_empty())
        .map(str::to_string)
}

/// A line that is only a code of two groups of four letters or digits, as Grok Build prints it
/// under "Confirm this code in your browser".
fn looks_like_device_code(line: &str) -> bool {
    let mut groups = line.split('-');
    let is_group = |group: Option<&str>| {
        group.is_some_and(|group| {
            group.len() == 4 && group.chars().all(|c| c.is_ascii_alphanumeric())
        })
    };
    is_group(groups.next()) && is_group(groups.next()) && groups.next().is_none()
}

/// Removes the colour and cursor sequences some CLIs print even into a pipe.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }
    out
}

/// A CLI running in a session of its own, with the lines of its stdout and stderr (colour
/// stripped, trimmed) arriving on [`Self::lines`]. A cancel ([`Self::stop`]) kills the whole
/// session, and so does dropping it, as a flow's end by a timeout or the daemon's exit does: some
/// helpers of an agent CLI move into process groups of their own, where a signal to its group
/// would miss them. A CLI that exits by itself is only reaped, its session left as it is: the
/// browser a login opens can be in it, started directly by `xdg-open` on a Linux desktop.
struct Process {
    child: Child,
    /// Names the CLI in the log of a session sweep: the agent's name.
    label: &'static str,
    stdin: Option<ChildStdin>,
    lines: mpsc::UnboundedReceiver<String>,
}

impl Process {
    fn spawn(
        mut command: std::process::Command,
        label: &'static str,
        pipe_stdin: bool,
    ) -> std::io::Result<Self> {
        process_session::lead_session(&mut command);
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
        let (tx, lines) = mpsc::unbounded_channel();
        if let Some(stdout) = child.stdout.take() {
            forward_lines(stdout, tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            forward_lines(stderr, tx);
        }
        Ok(Self {
            stdin: child.stdin.take(),
            child,
            label,
            lines,
        })
    }

    /// Writes `request` as a line to the CLI's stdin.
    async fn send(&mut self, request: &Value) -> std::io::Result<()> {
        let stdin = self.stdin.as_mut().ok_or(std::io::ErrorKind::BrokenPipe)?;
        stdin.write_all(format!("{request}\n").as_bytes()).await?;
        stdin.flush().await
    }

    /// Reads on until Codex's `account/login/completed`, or until the CLI's output ends.
    async fn until_completed(&mut self) {
        while let Some(line) = self.lines.recv().await {
            let completed = serde_json::from_str::<Value>(&line).is_ok_and(|message| {
                message.get("method").and_then(Value::as_str) == Some("account/login/completed")
            });
            if completed {
                return;
            }
        }
    }

    /// Kills the CLI's whole session and waits for the CLI to be gone, so that what it held
    /// (Codex's login port) is free when this returns. The session is swept once the CLI has
    /// exited, found out without reaping it (`WNOWAIT`), while its zombie keeps the session id
    /// from passing to another process.
    async fn stop(&mut self) {
        // `id` is `None` once the child has been reaped, when its pid may be someone else's.
        if let Some(pid) = self.child.id() {
            let pid = pid as libc::pid_t;
            process_session::kill_group(pid);
            loop {
                match process_session::exited_unreaped(pid) {
                    Some(false) => tokio::time::sleep(EXIT_POLL).await,
                    Some(true) => {
                        process_session::kill_members(pid).log(pid, self.label);
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
    async fn last_words(&mut self, last_words: &mut String) {
        while let Ok(Some(line)) = tokio::time::timeout(LAST_WORDS, self.lines.recv()).await {
            if !line.is_empty() {
                *last_words = line;
            }
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // Swept without waiting for the CLI to exit, which a drop cannot do, so a process it
        // starts while the kill is landing is missed.
        if let Some(pid) = self.child.id() {
            let pid = pid as libc::pid_t;
            process_session::kill_group(pid);
            process_session::kill_members(pid).log(pid, self.label);
        }
    }
}

fn forward_lines(
    pipe: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    tx: mpsc::UnboundedSender<String>,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(pipe);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            // Read as bytes and decoded lossily: a line that is not UTF-8 must not stop the pipe
            // from being read, as a full one stalls the CLI.
            match reader.read_until(b'\n', &mut bytes).await {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            let line = String::from_utf8_lossy(&bytes);
            if tx.send(strip_ansi(&line).trim().to_string()).is_err() {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        assert_stops_running, helper_in_its_own_group, is_running, read_pid_file, KillOnDrop,
        ScratchDir, ScratchFile, PATIENCE,
    };

    #[test]
    fn grok_builds_its_page_and_code_from_the_lines_it_prints() {
        let url = "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234";
        assert_eq!(url_in(&format!("  {url}")).as_deref(), Some(url),);
        assert_eq!(url_in("To sign in, open this URL in your browser:"), None);
        assert_eq!(user_code_in(url).as_deref(), Some("ABCD-1234"));
        assert_eq!(user_code_in("https://accounts.x.ai/oauth2/device"), None);
        assert!(looks_like_device_code("ABCD-1234"));
        assert!(!looks_like_device_code("Waiting for authorization..."));
        assert!(!looks_like_device_code("ABCD-12345"));
        assert!(!looks_like_device_code("abcd-12-34"));
    }

    #[tokio::test]
    async fn a_line_that_is_not_utf8_does_not_stop_the_lines_after_it() {
        let (tx, mut lines) = mpsc::unbounded_channel();
        forward_lines(&b"bad \xff line\n\x1b[1mnext\x1b[0m\nlast"[..], tx);
        let mut got = Vec::new();
        while let Some(line) = lines.recv().await {
            got.push(line);
        }
        assert_eq!(got, ["bad \u{fffd} line", "next", "last"]);
    }

    #[test]
    fn colour_sequences_are_taken_out_of_what_a_cli_prints() {
        assert_eq!(
            strip_ansi("\u{1b}[1;32mABCDE-12345\u{1b}[0m"),
            "ABCDE-12345"
        );
        assert_eq!(strip_ansi("plain ✓"), "plain ✓");
    }

    #[test]
    fn codex_lines_are_told_apart_by_their_ids_and_methods() {
        let started: Value = serde_json::from_str(
            r#"{"id":2,"result":{"type":"chatgpt","loginId":"L","authUrl":"https://a/b"}}"#,
        )
        .unwrap();
        assert!(matches!(
            codex_event(&started, None),
            CodexEvent::Started { login, url } if login == "L" && url == "https://a/b"
        ));
        let refused: Value =
            serde_json::from_str(r#"{"id":2,"error":{"code":-32600,"message":"busy"}}"#).unwrap();
        assert!(matches!(codex_event(&refused, None), CodexEvent::Rejected(m) if m == "busy"));
        let done = |extra: &str| -> Value {
            serde_json::from_str(&format!(
                r#"{{"method":"account/login/completed","params":{{"loginId":"L",{extra}}}}}"#
            ))
            .unwrap()
        };
        assert!(matches!(
            codex_event(&done(r#""success":true"#), Some("L")),
            CodexEvent::Completed
        ));
        assert!(matches!(
            codex_event(&done(r#""success":false,"error":"no""#), Some("L")),
            CodexEvent::GaveUp(m) if m == "no"
        ));
        // A message with a method is the server's own, whatever id it numbers itself by.
        let request: Value =
            serde_json::from_str(r#"{"id":2,"method":"item/tool/call","params":{}}"#).unwrap();
        assert!(matches!(codex_event(&request, None), CodexEvent::Other));
        // Another login's end is not this one's.
        assert!(matches!(
            codex_event(&done(r#""success":true"#), Some("other")),
            CodexEvent::Other
        ));
        let read = |account: &str| -> Value {
            serde_json::from_str(&format!(r#"{{"id":3,"result":{{"account":{account}}}}}"#))
                .unwrap()
        };
        assert!(matches!(
            codex_event(&read(r#"{"type":"chatgpt"}"#), None),
            CodexEvent::Confirmed(true)
        ));
        assert!(matches!(
            codex_event(&read("null"), None),
            CodexEvent::Confirmed(false)
        ));
    }

    #[test]
    fn the_browser_script_appends_its_argument_to_the_file_the_flow_watches() {
        let dir = crate::test_support::ScratchDir::new("login-browser-script");
        let flow = Flow {
            agent: Agent::Claude,
            shell_env: HashMap::new(),
            config_dir: None,
            resolved_dir: None,
            binary: String::new(),
            scratch: Scratch(dir.join("it's here")),
        };
        let (script, urls) = flow.write_browser_script().unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&flow.scratch.0)
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        for url in ["https://a/one?x=1&y=2", "https://a/two"] {
            // A process forked by another thread while the script was open for writing holds it
            // open until it execs, and until then the script cannot be run (`ETXTBSY`).
            let deadline = std::time::Instant::now() + PATIENCE;
            let status = loop {
                match std::process::Command::new(&script).arg(url).status() {
                    Err(err)
                        if err.raw_os_error() == Some(libc::ETXTBSY)
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    other => break other.unwrap(),
                }
            };
            assert!(status.success());
        }
        assert_eq!(last_line_of(&urls), "https://a/two");
        assert_eq!(
            std::fs::read_to_string(&urls).unwrap(),
            "https://a/one?x=1&y=2\nhttps://a/two\n"
        );
    }

    /// Starts a stand-in login CLI that leaves a helper in a process group of its own and then
    /// runs `then`, and returns it with the helper's pid once the helper is running.
    async fn stand_in_with_a_helper(then: &str) -> (Process, libc::pid_t) {
        let pid_file = ScratchFile::new("login-helper", "pid");
        let mut command = std::process::Command::new("/bin/bash");
        command.args([
            "-c",
            &format!("{}; {then}", helper_in_its_own_group(&pid_file)),
        ]);
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

    #[tokio::test]
    async fn a_login_cli_stopped_or_dropped_leaves_no_helper_behind() {
        for stopped in [true, false] {
            let (mut process, helper) = stand_in_with_a_helper("exec /bin/sleep 300").await;
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
        let (mut process, helper) = stand_in_with_a_helper("exit 0").await;
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
        let flow = Flow {
            agent: Agent::Codex,
            binary: String::new(),
            shell_env: HashMap::new(),
            config_dir: None,
            resolved_dir: None,
            scratch: Scratch(scratch.to_path_buf()),
        };
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "echo 'Error: bad configuration' >&2; exit 1"]);
        let mut process = Process::spawn(command, "stand-in", true).expect("the stand-in starts");
        // Exited, found out without reaping it, so the write below meets a closed pipe.
        let pid = process.child.id().expect("not reaped") as libc::pid_t;
        let deadline = tokio::time::Instant::now() + PATIENCE;
        while process_session::exited_unreaped(pid) != Some(true) {
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
