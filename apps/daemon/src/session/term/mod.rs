//! Launching an agent in a PTY, and writing messages into a running one.

mod image_path;
mod invisible;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::adapter::{self, LaunchSpec};
use crate::env_shell;
use crate::paths;
use crate::platform::term::{self as host_term, Input, PartialWrite};
use crate::protocol::{error_code, AgentId, CodedError, Notice, SessionKind, TerminalAppearance};
use crate::providers::ProviderSettings;
use crate::session::{self, LiveSession, NewSession};
use crate::session_git_hooks;
use crate::terminal::backend::{self, Helpers, Launching, LocalHelpers};
use crate::terminal::pty;

pub struct Launch {
    pub session: Arc<LiveSession>,
    /// The agent's own session id when the adapter could pre-allocate it. Codex cannot, and learns
    /// it from its first hook payload instead.
    pub agent_session_id: Option<String>,
    /// Something about this launch the user has to be told; see `adapter::LaunchPlan::notice`.
    pub notice: Option<Notice>,
}

/// One agent process to start. Owned rather than borrowed: launching snapshots the user's shell
/// environment and forks, so it runs on a blocking thread and the values have to travel to it.
pub struct LaunchRequest {
    pub session_id: String,
    pub agent: AgentId,
    pub kind: SessionKind,
    /// Who the session reports to, if anyone; see `LaunchSpec::owner`.
    pub owner: Option<crate::session_tools::Owner>,
    /// The project's effective git workflow settings; see `LaunchSpec::git_workflow`.
    pub git_workflow: Option<crate::protocol::GitWorkflowSettings>,
    /// See `LaunchSpec::ask_questions_as_choices`.
    pub ask_questions_as_choices: bool,
    /// See `LaunchSpec::user_language`.
    pub user_language: Option<String>,
    pub cwd: PathBuf,
    /// Initial task, only on a fresh launch.
    pub task: Option<String>,
    /// Set when this relaunches an existing agent conversation rather than starting a new one.
    pub resume_agent_session_id: Option<String>,
    /// The configuration directory of the session's own agent that it is pinned to; see
    /// `protocol::Session::config_dir`.
    pub config_dir: Option<PathBuf>,
    /// See `LaunchSpec::settings`.
    pub settings: ProviderSettings,
    /// The default account's directory, fully resolved, when the session's account shares it;
    /// see `LaunchSpec::shared_default_dir`.
    pub shared_default_dir: Option<PathBuf>,
    /// The model to launch with; see `LaunchSpec::model`.
    pub model: Option<String>,
    /// The reasoning effort to launch with; see `LaunchSpec::effort`.
    pub effort: Option<String>,
    /// See `LaunchSpec::terminal_appearance`.
    pub terminal_appearance: Option<TerminalAppearance>,
    /// A shell environment already snapshotted for this launch, used as it is instead of taking
    /// another. Only a caller that has already resolved something from one passes it — a switch
    /// to the default account, which resolved that account's directory from it to copy the
    /// conversation there and must launch against the same value.
    pub preresolved_shell_env: Option<HashMap<String, String>>,
    /// The daemon's own port, which the launch's helpers call when the program runs in this
    /// process; a program run by a holder is given the holder's instead (see `backend::Helpers`).
    pub daemon_port: u16,
    /// The daemon's own key, which the launch's helpers carry; see `daemon_port`.
    pub daemon_key: String,
    /// The daemon's own executable, which the launch's helpers run; see `daemon_port`.
    pub self_exe: String,
    pub mcp_token: String,
    /// What every process of this launch carries, whatever the agent and the session's kind; see
    /// `crate::processes::ledger`.
    pub process_tag: crate::processes::ledger::ProcessTag,
}

/// Starts one agent process. A resume assembles the full injection just as a fresh launch does,
/// because hooks and the MCP server are resolved from the launch arguments every time and a resume
/// that omits them silently produces a session nobody observes.
pub fn launch(request: LaunchRequest) -> Result<Launch> {
    let LaunchRequest {
        session_id,
        agent,
        kind,
        owner,
        git_workflow,
        ask_questions_as_choices,
        user_language,
        cwd,
        task,
        resume_agent_session_id,
        config_dir,
        settings,
        shared_default_dir,
        model,
        effort,
        terminal_appearance,
        preresolved_shell_env,
        daemon_port,
        daemon_key,
        self_exe,
        mcp_token,
        process_tag,
    } = request;
    let session_id = session_id.as_str();
    let cwd = cwd.as_path();
    if !cwd.is_dir() {
        // The terminal backend may fall back to the home directory for a cwd that does not exist,
        // which would silently start a write-capable agent in `$HOME`.
        return Err(CodedError::raised(
            error_code::DIRECTORY_UNREACHABLE,
            format!(
                "`{}` is not a directory the daemon can reach. A packaged application needs file \
                 access granted per volume, and the prompt may not have been answered yet.",
                cwd.display()
            ),
            &[("path", &cwd.to_string_lossy())],
        ));
    }

    let shell_env = match preresolved_shell_env {
        Some(shell_env) => shell_env,
        None => {
            env_shell::launch_snapshot().context("snapshotting the user's shell environment")?
        }
    };

    let scratch = paths::session_scratch(session_id);
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch).ok();
    }
    std::fs::create_dir_all(&scratch).with_context(|| format!("creating {}", scratch.display()))?;

    let adapter = adapter::provider(agent);
    let capabilities = adapter.descriptor().capabilities;
    // Asked for before anything names it: the holder that is to run the program issues the key its
    // helpers carry. Dropped unused if the launch fails before the program starts, which revokes
    // the key.
    let helpers = Helpers::issue(
        session_id,
        &mcp_token,
        LocalHelpers {
            port: daemon_port,
            key: &daemon_key,
            exe: &self_exe,
        },
    )?;
    let hook_script = if capabilities.hook_script {
        adapter::write_hook_script(
            &scratch,
            &helpers.exe,
            session_id,
            helpers.port,
            &helpers.key,
        )
        .context("writing the session's hook script")?
    } else {
        adapter::hook_script_path(&scratch)
    };

    let new_agent_session_id = Uuid::new_v4().to_string();
    // Resolved ahead of the plan because the agent's version, read from where the binary is
    // installed, decides how some adapters inject; a failure is raised only after the plan, so a
    // plan's own error (a missing pinned directory, say) is the one the user is told about.
    let binary = adapter::resolve_binary(agent, &shell_env);
    let spec = LaunchSpec {
        session_id,
        kind,
        owner,
        git_workflow,
        ask_questions_as_choices,
        user_language: user_language.as_deref(),
        new_agent_session_id: &new_agent_session_id,
        resume_agent_session_id: resume_agent_session_id.as_deref(),
        cwd,
        task: task.as_deref(),
        scratch: &scratch,
        hook_script: &hook_script,
        shell_env: &shell_env,
        config_dir: config_dir.as_deref(),
        settings: &settings,
        compatibility_sources: adapter::compatibility_sources_of(agent, &settings),
        shared_default_dir: shared_default_dir.as_deref(),
        model: model.as_deref(),
        effort: effort.as_deref(),
        terminal_appearance,
        agent_version: binary.as_ref().ok().and_then(|binary| {
            adapter
                .installed_version(&binary.program.to_string_lossy())
                .or_else(|| {
                    adapter::version_from_output(&crate::platform::shell::probe_version(
                        binary, &shell_env,
                    )?)
                })
        }),
        self_exe: &helpers.exe,
        daemon_port: helpers.port,
        daemon_key: &helpers.key,
        mcp_token: &mcp_token,
    };
    let plan = adapter.plan(&spec)?;
    // A stash key is used only where the provider says this launch was measured to take it.
    let native_stash = adapter.screen().native_stash(&spec, &plan);

    let binary = binary?;

    let mut environment = shell_env.clone();
    environment.extend(plan.env.iter().cloned());
    if kind == SessionKind::Project {
        environment.extend(session_git_hooks::session_env(
            &shell_env,
            &helpers.exe,
            session_id,
            helpers.port,
            &helpers.key,
        ));
    }
    process_tag.containment().prepare(&mut environment);
    environment.insert("TERM".to_string(), pty::TERM.to_string());
    // Where the plan's private server runs: none where the holder that is to run the terminal
    // runs no companions, and the launch is then the ordinary one.
    let host = helpers.server_host(
        process_tag.containment(),
        process_tag.instance_id(),
        process_tag.launch_id(),
    );
    let (args, transport, companion) = match (&plan.private_server, &host) {
        (Some(server), Some(host)) => {
            match server.start(&binary, &environment, cwd, &**host, process_tag.launch_id()) {
                Ok(started) => {
                    environment.extend(started.env);
                    (started.args, Some(started.transport), host.companion())
                }
                // The ordinary launch stands on its own: a server that does not come up costs the
                // session only the delivery that would have gone through it.
                Err(err) => {
                    tracing::warn!(
                        session = %session_id,
                        error = %format!("{err:#}"),
                        "the agent's own server could not be started; launching its terminal on its own"
                    );
                    host.end();
                    (plan.args.clone(), None, false)
                }
            }
        }
        (Some(_), None) => {
            tracing::info!(
                session = %session_id,
                "the holder runs no companions; launching the agent's terminal without its own \
                 server"
            );
            (plan.args.clone(), None, false)
        }
        (None, _) => (plan.args.clone(), None, false),
    };
    // It borrows the launch's credentials, which the spawn takes.
    drop(host);
    let mut cmd = host_term::Command::new(&binary.program);
    cmd.args(&binary.leading_args);
    cmd.args(&args);
    cmd.env_clear();
    for (key, value) in &environment {
        cmd.env(key, value);
    }
    cmd.cwd(cwd);
    if let Some(keys) = adapter.descriptor().quit_keys {
        cmd.quit_keys(keys);
    }

    // What a holder keeps of the session, so that a daemon that starts later can tell what the
    // terminal is and rebuild the session around it (`crate::coordinator::adopt`).
    let launch_prompt = task.as_deref().and_then(launch_prompt);
    let meta = serde_json::json!({
        "kind": "session",
        "session_id": session_id,
        "launch_id": process_tag.launch_id(),
        "agent": agent,
        "scratch": scratch.to_string_lossy(),
        // What a daemon that adopts the terminal restores its map of MCP tokens from.
        "mcp_token": mcp_token,
        "resolves_approvals_itself": plan.resolves_approvals_itself,
        "native_stash": native_stash,
        "launch_prompt": launch_prompt,
        "adoption": plan.adoption,
    });
    let spawned = backend::start(Launching {
        command: cmd,
        size: pty::INITIAL_SIZE,
        containment: process_tag.containment(),
        instance: process_tag.instance_id(),
        launch: process_tag.launch_id(),
        // As `LiveSession::new` has its terminal's core treat the process.
        kill_session_leftovers: true,
        meta,
        helpers: Some(helpers),
        companion,
    })
    .with_context(|| format!("spawning `{}`", binary.program.display()))?;

    let agent_session_id = match resume_agent_session_id {
        Some(id) => Some(id),
        None if capabilities.preallocates_session_id => Some(new_agent_session_id),
        None => None,
    };

    let live = Arc::new(LiveSession::new(NewSession {
        id: session_id.to_string(),
        agent,
        launch_id: process_tag.launch_id().to_string(),
        spawned,
        checks_composer: true,
        transport: transport.clone(),
        scratch_dir: Some(scratch),
        resolves_approvals_itself: plan.resolves_approvals_itself,
        carried_trust: plan.carried_trust,
        before_scratch_removal: before_scratch_removal(transport, plan.after_exit),
    }));
    live.enable_native_stash(native_stash);
    if let Some(prompt) = launch_prompt {
        live.note_launch_prompt(prompt);
    }
    session::spawn_reader_thread(live.clone(), pty::READ_BUF);

    Ok(Launch {
        session: live,
        agent_session_id,
        notice: plan.notice,
    })
}

/// A server of the provider's own that a session's automatic messages go through instead of its
/// terminal.
pub trait MessageTransport: Send + Sync {
    /// Delivers `text` as the message `logical_id`. An error whose `written` is 0 took nothing in;
    /// one carrying [`UncertainSubmission`] may have.
    fn send(&self, text: &str, logical_id: &str) -> Result<Sent, PartialWrite>;
    /// Connects the server's events to the running session, once it is registered; an event that
    /// came before is passed on then.
    fn attach(&self, events: TransportEvents);
    /// Has the server run the messages it took that wait in its queue, as it does not by itself
    /// after a turn the user or an owner cancelled, and says what came of it.
    fn start_queue(&self) -> QueueStart;
    /// Ends the server where the daemon runs it, and withdraws what the session was told of
    /// messages held back; called after the session's process is reaped. A server a holder runs
    /// with the terminal is the holder's to end, and is left to it.
    fn stop(&self);
}

/// What came of a [`MessageTransport::send`] that did not fail.
#[derive(Debug, PartialEq, Eq)]
pub enum Sent {
    /// The server took the message.
    Queued,
    /// The server did not take it and nothing of it reached the server, for the reason given; the
    /// terminal can take it instead, unless a planned stop's handover barrier is up (see
    /// [`send_message_with_id`]).
    Declined(String),
}

/// What a session's own server tells the session, through [`MessageTransport::attach`].
pub struct TransportEvents {
    /// The server can now take a message it had to hold back, so the session's queue is to be
    /// written out again.
    pub ready: Box<dyn Fn() + Send + Sync>,
    /// The server ended while the session ran: its terminal, which depends on it, is to end too.
    pub server_gone: Box<dyn Fn() + Send + Sync>,
    /// Messages have waited long for the server to take them, and the user is to be told this
    /// notice about the session.
    pub held: Box<dyn Fn(Notice) + Send + Sync>,
    /// The server took them after all: the notice of this code that [`Self::held`] gave is
    /// withdrawn.
    pub released: Box<dyn Fn(crate::protocol::Code) + Send + Sync>,
}

/// What came of [`MessageTransport::start_queue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueStart {
    /// None of the server's messages waited in its queue.
    Nothing,
    /// This many of them waited and now run, or ran just before, one turn each.
    Started(usize),
    /// This many of them wait behind messages someone else queued, so nothing was started ahead
    /// of those.
    BehindOthers(usize),
    /// Whether any waited, and what became of them, could not be told, for this reason.
    Failed(String),
}

/// The agent's own queue refused a message outright, for the reason given: nothing of it was
/// taken, and it is not sent again by any route.
#[derive(Debug)]
pub struct RefusedByAgent(pub String);

impl std::fmt::Display for RefusedByAgent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RefusedByAgent {}

/// The session's own server holds a message back for now, for the reason given: nothing of it was
/// taken, and it is written again later, by the server or, failing it, the terminal.
#[derive(Debug)]
pub struct HeldByServer(pub String);

impl std::fmt::Display for HeldByServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for HeldByServer {}

/// The way a message went into the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Into the queue of the session's own server, which runs it as a turn of its own.
    Queued,
    /// Into its terminal.
    Terminal,
}

/// A started [`PrivateServer`]: what the terminal is started with and the transport to the server.
pub struct StartedServer {
    /// Replaces `LaunchPlan::args`.
    pub args: Vec<String>,
    /// Added to the terminal's environment.
    pub env: Vec<(String, String)>,
    pub transport: Arc<dyn MessageTransport>,
}

/// How a launch starts such a server, when its plan has one.
pub trait PrivateServer: Send + Sync {
    /// Starts the server for the session of the launch `launch_id` through `host`, with the
    /// launch's environment, before its terminal. **Blocks.**
    fn start(
        &self,
        binary: &crate::platform::shell::ResolvedBinary,
        environment: &HashMap<String, String>,
        cwd: &Path,
        host: &dyn crate::terminal::server_host::ServerHost,
        launch_id: &str,
    ) -> Result<StartedServer>;

    /// The plan itself, for the tests of the provider that made it.
    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any;
}

/// What a session does before its scratch directory is removed: stops its `transport` and runs its
/// `after_exit`; `None` where it has neither.
pub fn before_scratch_removal(
    transport: Option<Arc<dyn MessageTransport>>,
    after_exit: Option<Box<dyn AfterExit>>,
) -> Option<Box<dyn FnOnce() + Send>> {
    (transport.is_some() || after_exit.is_some()).then(move || {
        Box::new(move || {
            if let Some(transport) = transport {
                transport.stop();
            }
            if let Some(after_exit) = after_exit {
                after_exit.run();
            }
        }) as Box<dyn FnOnce() + Send>
    })
}

/// Work a launch leaves for once its process has exited (`LaunchPlan::after_exit`), done before the
/// session's scratch directory is removed. It cannot fail the session, which has already ended, so
/// it reports nothing back.
pub trait AfterExit: Send + Sync {
    fn run(&self);

    /// The work itself, for the tests of the provider that left it.
    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any;
}

/// What is said about a message the agent only partly accepted, to the sender whose call hit it and
/// to the user. One wording, because they are the same fact seen from two places. It holds for a
/// partial write that leaves the session's later writes free; one made while a draft was stashed
/// holds them (`crate::session::HeldFragment`) and is worded by [`HELD_FRAGMENT`].
///
/// It names no reader and promises no recovery: what state the input line is actually left in, and
/// what it takes to clear it, is not established for any of the three agents. What *is* known follows
/// from the bracketed-paste contract this whole mechanism already rests on — a receiver accumulates
/// until the end marker, which is why a multi-line message arrives as one — so the next message
/// Octoboard writes closes the fragment and is submitted along with it. The damage is one spoiled
/// message, not a session that stays broken.
pub const FRAGMENT_HAZARD: &str = concat!(
    "the session's input line may be holding part of a message ",
    env!("OCTOBOARD_APP_NAME"),
    " could not finish writing. The next message written into the session will be run \
     together with it, so check the session before sending anything else."
);

/// [`FRAGMENT_HAZARD`] for a session that holds its later writes behind the fragment until the user
/// sends a prompt, which submits the input line: nothing is run together with it, but a message sent
/// meanwhile waits.
pub const HELD_FRAGMENT: &str = concat!(
    "the session's input box may be holding part of it. ",
    env!("OCTOBOARD_APP_NAME"),
    " writes nothing more into the session until a prompt is sent, so a message \
     sent now waits until then."
);

/// Acceptance could not be established after the queue request was sent. Retrying could duplicate
/// it.
#[derive(Debug)]
pub struct UncertainSubmission(pub String);

impl std::fmt::Display for UncertainSubmission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UncertainSubmission {}

#[cfg(test)]
pub fn send_message(session: &LiveSession, text: &str) -> Result<(), PartialWrite> {
    send_message_with_id(session, text, &Uuid::new_v4().to_string(), false, &|| false).map(|_| ())
}

/// Submits a message through the session's own server ([`MessageTransport`]) where it has one, or
/// by checked bracketed paste + Enter.
///
/// On the terminal path, a failure's `written` counts bytes of the **framed** message as the
/// terminal encodes the paste and its Enter, not of `text`, so zero is
/// the one case where nothing of the message reached the session. Any other count may have left an
/// unterminated paste in the input line, or — for a count short of the paste introducer — a
/// truncated escape sequence; either way the message may not be written again.
///
/// Where the server may have accepted the message without confirming it, the failure carries
/// [`UncertainSubmission`] and nothing is written into the terminal, which could deliver it twice.
/// Where nothing of it reached the server, the terminal path below takes it, unless
/// `writes_stopped` says a planned stop has raised its handover barrier since the message was
/// taken: then nothing is written and the failure is an empty [`PartialWrite`], which the outbox
/// puts back at the front of its queue for the stop to save for the next daemon, as it does with
/// every message the barrier stops.
/// The input write checks for an empty, acknowledged composer or saves and restores the draft
/// through a supported native stash. Preservation has a bounded grace period; after expiry,
/// delivery may include unfinished user input and retained keys are released into the current editor.
/// The caller also holds messages while a modal dialog is up,
/// because the paste is discarded but the trailing Enter confirms whatever option is highlighted —
/// at Claude Code's trust dialog that exits the session, and at Grok's approval modal it selects
/// always-approve. Writing mid-turn is safe and is the better path: the agent takes the message
/// itself, queuing it until the turn ends or, for Codex in its terminal, steering it into the
/// running turn; Codex's own queue runs it as a turn of its own after the running one.
pub fn send_message_with_id(
    session: &LiveSession,
    text: &str,
    logical_id: &str,
    allow_mixed: bool,
    writes_stopped: &dyn Fn() -> bool,
) -> Result<Route, PartialWrite> {
    if let Some(transport) = &session.transport {
        if session.is_frozen()
            || session.trust.holds_writes()
            || session.look_for_dialog().current.is_some()
        {
            return Err(PartialWrite {
                written: 0,
                error: std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "the session's terminal is frozen, or shows its trust confirmation or a dialog",
                ),
            });
        }
        match transport.send(&shaped(text), logical_id)? {
            Sent::Queued => return Ok(Route::Queued),
            // Nothing of the message reached the server. A send begun before the handover barrier
            // can end after it rose, and from the barrier on the outbox writes nothing: the
            // message stays whole for the next daemon rather than being written.
            Sent::Declined(reason) if writes_stopped() => {
                tracing::info!(
                    session = %session.id,
                    %reason,
                    "the agent's own server did not take a message after the handover barrier \
                     was raised; it is kept for the next daemon"
                );
                return Err(kept_for_next_daemon());
            }
            // Otherwise the terminal takes it instead.
            Sent::Declined(reason) => tracing::warn!(
                session = %session.id,
                %reason,
                "the agent's own server did not take a message; writing it into the terminal"
            ),
        }
    }
    let framed = frame_message(session.agent, text);
    let [Input::Paste(prompt), _] = &framed else {
        unreachable!("a message is framed as a paste");
    };
    session
        .write_message_input(&framed, prompt, allow_mixed)
        .map(|()| Route::Terminal)
}

/// The failure of a message [`send_message_with_id`] does not write because the handover barrier is
/// up: nothing of it went in, so the outbox puts it back.
fn kept_for_next_daemon() -> PartialWrite {
    PartialWrite {
        written: 0,
        error: std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "a planned stop is handing the session over; the message is kept for the next daemon",
        ),
    }
}

/// Shapes a message into the inputs `send_message_with_id` writes on the terminal path: a paste of
/// the message, made as [`shaped`] says and with whatever the receiving agent needs on top, then
/// Enter. Written as one buffer, which is what makes a multi-line message arrive as a single
/// submitted message with no delay needed between the paste and the Enter, except where the
/// terminal hands the agent the paste as keys and the agent would read that Enter as part of it
/// (`crate::session::paste_apart` writes the Enter apart there). Pulled out as a pure function so
/// the ordering — both strips before the slash guard, not after — is something a test can pin
/// directly, rather than re-deriving it from a copy of this logic; the send path itself needs a
/// `LiveSession` and a PTY, which is why it is not tested directly.
pub(crate) fn frame_message(agent: AgentId, text: &str) -> [Input; 2] {
    let text = shaped(text);
    // An agent that reads a pasted piece ending in an image extension as an image loses the Enter
    // that follows (see `image_path`).
    let text = if crate::adapter::provider(agent)
        .screen()
        .pasted_image_path_loses_enter()
    {
        image_path::guard_image_paths(&text)
    } else {
        text
    };
    [Input::Paste(text), Input::Enter]
}

/// A message as the agent should read it: control and invisible characters stripped, then a
/// leading slash guarded.
fn shaped(text: &str) -> String {
    // `text` can be model-authored (a report panel submission, a project session's report summary)
    // and is never reviewed before it is written here. Without this, a value containing the
    // paste-end marker (`ESC[201~`) would close the bracketed paste early, and everything the
    // attacker put after it would be delivered to the agent's TUI as raw input — control
    // sequences and `\r` included, i.e. arbitrary keystrokes. Stripping every Cc control character
    // removes ESC along with it, so no embedded escape sequence can survive into the framed
    // buffer.
    //
    // This has to run before the slash-command check below: a control character prepended to
    // `/clear` makes the raw text not start with `/`, but stripping it bare would still leave
    // `/clear` for the agent to execute as a command. Checking the stripped text instead closes
    // that gap.
    //
    // The invisible characters go for a different reason: Claude Code holds back a prompt that
    // contains any, and the Enter written after the paste does not send it (see `invisible`).
    // They are stripped before the slash check for the same reason as the controls.
    let text = stripped(text);

    // A message starting with `/` is executed as a slash command even inside a bracketed paste, so
    // it never reaches the model; a single leading space makes it arrive as ordinary text.
    if text.starts_with('/') {
        format!(" {text}")
    } else {
        text
    }
}

/// A message as the command line of an agent launch takes it, or `None` when nothing is left of it.
/// It gets [`shaped`] (the invisible-character strip and the slash guard) but not the image-path
/// guard of `frame_message`, which is for a paste: argv is not one. A prompt that began with `/`
/// would run a slash command, as a pasted one would, and one that began with `-` would be read as
/// a flag; a leading space keeps either as text.
pub(crate) fn launch_prompt(text: &str) -> Option<String> {
    if is_blank_once_stripped(text) {
        return None;
    }
    let text = shaped(text);
    Some(if text.starts_with('-') {
        format!(" {text}")
    } else {
        text
    })
}

/// Whether nothing but whitespace is left of a message once its control and invisible characters
/// are stripped. Such a message would be written as an empty prompt, which no agent sends.
pub(crate) fn is_blank_once_stripped(text: &str) -> bool {
    stripped(text).trim().is_empty()
}

/// The text of a message with what `frame_message` strips taken out.
pub(crate) fn stripped(text: &str) -> String {
    invisible::strip_invisible(&strip_control_chars(text))
}

/// Removes every Unicode `Cc` control character — C0, DEL and C1 — except `\n` and `\t`, which
/// prose legitimately uses. See the comment at `frame_message`'s call site for why this exists.
fn strip_control_chars(text: &str) -> String {
    text.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bracketed paste around a message and the Enter after it, as the bytes a Unix terminal
    /// receives them in.
    const PASTE_START: &[u8] = b"\x1b[200~";
    const PASTE_END: &[u8] = b"\x1b[201~";
    const SUBMIT: &[u8] = b"\r";

    /// The buffer `super::frame_message`'s inputs are written into the terminal as.
    fn frame_message(agent: AgentId, text: &str) -> Vec<u8> {
        host_term::encode(&super::frame_message(agent, text))
    }

    /// An embedded paste-end marker must not survive into the framed buffer at all — not just be
    /// absent from the stripped text — since the buffer written into the PTY is what an attacker
    /// would actually need to end the paste early. `frame_message` adds its own closing
    /// `PASTE_END`, so this counts occurrences in the whole buffer rather than just checking the
    /// marker's absence from the input.
    #[test]
    fn an_embedded_paste_end_marker_does_not_survive() {
        let text = "before\x1b[201~\rafter";
        let framed = frame_message(AgentId::CLAUDE, text);
        let occurrences = framed
            .windows(PASTE_END.len())
            .filter(|window| *window == PASTE_END)
            .count();
        assert_eq!(
            occurrences, 1,
            "exactly the trailing PASTE_END, none embedded"
        );
    }

    #[test]
    fn a_message_of_only_controls_invisible_characters_and_spaces_is_blank() {
        assert!(is_blank_once_stripped(
            "\u{2068}\u{2069}\u{1b}\u{7f} \n\t\u{200B}"
        ));
        assert!(!is_blank_once_stripped("\u{2068}ok\u{2069}"));
        assert!(!is_blank_once_stripped("\u{1F9D1}"));
    }

    #[test]
    fn ordinary_multiline_text_with_tabs_survives_framing() {
        let text = "line one\n\tindented line two\nline three";
        let framed = frame_message(AgentId::CLAUDE, text);
        let mut expected = Vec::new();
        expected.extend_from_slice(PASTE_START);
        expected.extend_from_slice(text.as_bytes());
        expected.extend_from_slice(PASTE_END);
        expected.extend_from_slice(SUBMIT);
        assert_eq!(framed, expected);
    }

    /// Claude Code holds back a prompt holding bidirectional isolates instead of sending it, so
    /// none may reach the framed buffer — and an isolate in front of a slash command must not keep
    /// it from being guarded, any more than a control character may.
    #[test]
    fn invisible_characters_are_stripped_before_the_slash_guard() {
        let framed = frame_message(AgentId::CLAUDE, "\u{2068}/clear\u{2069} the cache");
        let mut expected = Vec::new();
        expected.extend_from_slice(PASTE_START);
        expected.extend_from_slice(b" /clear the cache");
        expected.extend_from_slice(PASTE_END);
        expected.extend_from_slice(SUBMIT);
        assert_eq!(framed, expected);
    }

    /// A control character prepended to a slash command must not let it slip past the
    /// leading-space guard: `frame_message` strips controls before checking for `/`, so the
    /// stripped text still gets space-prefixed and reaches the model as text, not as a command.
    /// Asserting on the framed buffer, rather than re-deriving the strip-then-guard order in the
    /// test body, is what would actually catch that order being swapped back in `frame_message`.
    #[test]
    fn a_control_character_before_a_slash_command_still_gets_guarded() {
        let text = "\u{1}/clear";
        let framed = frame_message(AgentId::CLAUDE, text);
        let mut expected = Vec::new();
        expected.extend_from_slice(PASTE_START);
        expected.extend_from_slice(b" /clear");
        expected.extend_from_slice(PASTE_END);
        expected.extend_from_slice(SUBMIT);
        assert_eq!(framed, expected);
    }

    /// A Codex stand-in whose own server declines every message and raises `barrier` while it
    /// does, as a send that ends after the handover barrier rose.
    fn declined_during(
        name: &str,
        barrier: &Arc<std::sync::atomic::AtomicBool>,
    ) -> (
        crate::test_support::StandIn,
        Arc<crate::test_support::DecliningServer>,
    ) {
        let raised = barrier.clone();
        let server = crate::test_support::DecliningServer::new(move || {
            raised.store(true, std::sync::atomic::Ordering::Release)
        });
        let live = crate::test_support::idle_stand_in_of(name, AgentId::CODEX)
            .with_transport(server.clone());
        (live, server)
    }

    /// Where the server declines a message and the barrier rose while it ran, nothing goes into
    /// the terminal, which is being handed over: the failure writes nothing and would block, which
    /// the outbox puts back for the stop to save for the next daemon.
    #[test]
    fn a_declined_message_is_not_written_once_the_barrier_rose_during_the_send() {
        let barrier = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (live, server) = declined_during("declined-after-barrier", &barrier);
        let before = live.input_writes();
        let failure = match send_message_with_id(&live, "hello", "launch:1", false, &|| {
            barrier.load(std::sync::atomic::Ordering::Acquire)
        }) {
            Err(failure) => failure,
            Ok(route) => panic!("the message went by {route:?}"),
        };
        assert_eq!(failure.written, 0);
        assert_eq!(failure.error.kind(), std::io::ErrorKind::WouldBlock);
        assert_eq!(live.input_writes(), before, "something was written");
        assert_eq!(server.offered().len(), 1);
        assert!(server.offered()[0].contains("hello"));
    }

    /// Before the barrier, a message the server declines still falls back to the terminal. The
    /// route does not depend on the agent; the stand-in is of one whose paste and Enter go in
    /// together on every host, so the write waits on no input box the stand-in never draws.
    #[test]
    fn a_declined_message_is_written_into_the_terminal_before_the_barrier() {
        let server = crate::test_support::DecliningServer::new(|| {});
        let live = crate::test_support::idle_stand_in("declined-before-barrier")
            .with_transport(server.clone());
        let before = live.input_writes();
        let route = send_message_with_id(&live, "hello", "launch:1", false, &|| false)
            .expect("the terminal takes it");
        assert_eq!(route, Route::Terminal);
        assert_eq!(live.input_writes(), before + 1);
        assert_eq!(server.offered().len(), 1);
    }

    #[test]
    fn only_a_claude_code_message_has_its_image_paths_wrapped() {
        let text = "done\n- /a/shot.png";
        let framed = |agent| String::from_utf8(frame_message(agent, text)).unwrap();
        assert!(framed(AgentId::CLAUDE).contains("- `/a/shot.png`\x1b[201~"));
        assert!(framed(AgentId::CODEX).contains("- /a/shot.png\x1b[201~"));
        assert!(framed(AgentId::GROK).contains("- /a/shot.png\x1b[201~"));
    }
}
