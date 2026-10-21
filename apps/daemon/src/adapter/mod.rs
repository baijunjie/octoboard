//! Agent adapters: one per agent CLI, each assembling the launch of that CLI so Octoboard's
//! capabilities are injected *additionally*, without modifying project files or the user's own
//! configuration. Everything else in the daemon is agent-agnostic.
//!
//! Three things are injected: the status hooks, the Octoboard MCP server, and a role description
//! ("What is injected on every launch" in `docs/product/launching-agents.md`). The mechanism for each
//! differs per agent and is the adapter's own business; the conditions all three share are below.
//!
//! Three rules hold for every adapter, each of which fails silently if broken:
//!
//! - **Resume re-injects everything.** For all three agents, hooks and the MCP server are resolved
//!   from the launch arguments every time and are lost on a resume that omits them, leaving a
//!   session nobody observes. Adapters therefore assemble the full injection on every launch,
//!   resume included.
//! - **A role cannot be changed after the first turn.** Claude Code records an appended system
//!   prompt on the conversation's first request and replays it verbatim afterwards, so different
//!   text passed on a later launch is silently ignored; Grok persists `--rules` into the session
//!   record. The role text is therefore a function of the session's role, the owner it had at
//!   launch and its agent alone, and names that owner: a later change of owner is told to the
//!   session in prose and replaces what the text says (a lead session, bound only after it was
//!   launched unbound, is launched as unbound every time). It is still passed on every launch,
//!   because Claude Code re-renders its snapshot from whatever *that* launch passed after a
//!   compaction.
//! - **Hooks must fail fast and silently.** Every agent surfaces a failing hook to the user, and a
//!   hook with no timeout blocks the turn for its full duration. Octoboard owns the hook command
//!   (`hook_command` below; Codex and Grok run it as the per-session `hook_script`, Claude Code
//!   inline, or not at all when its version posts hooks over HTTP itself), which exits 0
//!   unconditionally and writes nothing.
//!
//! The hook command takes no event argument: it derives the event from the payload on stdin, so
//! one constant command serves every event, and adding or removing an event never changes what an
//! adapter has to generate beyond the event name itself.

pub mod claude;
pub mod codex;
pub mod grok;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use crate::mcp::Owner;
use crate::protocol::{
    error_code, Agent, ApprovalReviewer, ClaudeApprovalMode, CodedError, Notice, Role,
    TerminalAppearance,
};

/// Everything an adapter needs to assemble one launch.
pub struct LaunchSpec<'a> {
    /// Octoboard's own session id. Not the agent's — it identifies the session to the daemon, and
    /// is what the injected MCP server is launched with.
    pub session_id: &'a str,
    /// Which side of the orchestration this session is on, which decides both the role text and
    /// the tools its MCP server announces.
    pub role: Role,
    /// Who this session reports to, if anyone, which decides what its role text tells it is
    /// waiting on it and which tools its MCP server announces. Never set for a console session.
    pub owner: Option<Owner>,
    /// The id to give an agent that can pre-allocate one for a *new* conversation. Freshly
    /// generated per launch: both Claude Code and Grok refuse an id that already has a stored
    /// conversation, so reusing one would make relaunching a session that was opened and never
    /// typed into fail with an opaque launch error.
    pub new_agent_session_id: &'a str,
    /// The agent's own session id, set when this launch resumes an existing conversation.
    pub resume_agent_session_id: Option<&'a str>,
    pub cwd: &'a Path,
    /// Initial task, rendered as the agent's positional prompt argument. Only on a fresh launch.
    pub task: Option<&'a str>,
    /// Per-session scratch directory, already created. Adapters may write into it whatever their
    /// injection needs; it is removed when the session's process is gone.
    pub scratch: &'a Path,
    /// Absolute path of the hook script for this session. Written only for an adapter whose
    /// [`AgentAdapter::uses_hook_script`] is true; for any other it is where the script would be,
    /// and nothing is there.
    pub hook_script: &'a Path,
    /// The filtered snapshot of the user's shell environment this launch will use. Adapters read
    /// the user's own settings from here rather than from the daemon's own environment, which may
    /// be that of an agent session the daemon happens to have been started from.
    pub shell_env: &'a HashMap<String, String>,
    /// The configuration directory of this session's own agent that it is pinned to, when it has
    /// one. Each adapter exposes it through its agent's own mechanism (`CLAUDE_CONFIG_DIR`,
    /// `CODEX_HOME`, or the directory Grok's per-session home is built from), layered over whatever
    /// the shell snapshot carries; absent leaves that snapshot untouched. An adapter reads it only
    /// through [`pinned_config_dir`], which refuses a directory that has vanished, and passes the
    /// result on to whatever else needs it.
    pub config_dir: Option<&'a Path>,
    /// Codex-only override; absent follows its own configuration.
    pub approval_reviewer: Option<ApprovalReviewer>,
    /// Claude Code-only override; absent follows its own configuration and passes no flag.
    pub approval_mode: Option<ClaudeApprovalMode>,
    /// The default account's directory, fully resolved, when the session's account shares the
    /// default configuration (`crate::shared_config`); `None` for every other launch. An adapter
    /// whose agent keeps per-project memory in its conversation directory points that memory
    /// here, so it lands where the default account keeps it.
    pub shared_default_dir: Option<&'a Path>,
    /// The model to launch with, in the agent's own spelling; `None` leaves it to the agent. Read
    /// from the setting for this agent of the session's console (a console session) or of its
    /// project (a project session), and `None` when that setting is unset.
    pub model: Option<&'a str>,
    /// The reasoning effort to launch with, in the agent's own spelling; `None` leaves it to the
    /// agent. Set under the same conditions as [`Self::model`].
    pub effort: Option<&'a str>,
    /// The appearance of the terminal this launch will be shown in. Only the Grok adapter reads
    /// it, to set `GROK_THEME`; absent means follow the operating system.
    pub terminal_appearance: Option<TerminalAppearance>,
    /// The version of the agent binary this launch starts, when it could be told without running
    /// it (see [`AgentAdapter::installed_version`]); `None` makes an adapter fall back to what
    /// every version takes.
    pub agent_version: Option<AgentVersion>,
    /// The daemon binary, which is also the MCP server and the hook forwarder.
    pub self_exe: &'a str,
    /// Where the daemon is listening, for the MCP server this launch registers.
    pub daemon_port: u16,
    /// The token that session's MCP server authenticates to the daemon with.
    pub mcp_token: &'a str,
}

/// What an agent's own time limit on a call to Octoboard's tools is set to, for every agent that
/// has such a limit: `mcp::stdio::CALL_TIMEOUT` plus a margin, so Octoboard's forwarding limit is
/// the one that ends a call rather than the agent's own default, whichever side of it that default
/// falls on. Derived from `CALL_TIMEOUT` so it stays above it if that is raised. The margin covers
/// a call in normal operation, not a daemon that stalls partway through a response: `CALL_TIMEOUT`
/// bounds each read, not the call. Why the ordering matters is in apps/daemon/PROTOCOL.md's
/// "`POST /mcp/:token`" section; what each agent does when its own limit passes, and so what the
/// setting prevents, is in that agent's section of `docs/agent-cli-reference.md`. Every agent takes
/// it per server, so no other MCP server of the user's is touched. Claude Code has none to set: it
/// moves a long call to the background rather than ending it.
///
/// Each adapter's tests pin the resulting number, so a change to `CALL_TIMEOUT` is not finished by
/// updating those tests: the new value has to hold for every agent, against what that agent's
/// section records about its own limit.
pub const MCP_TOOL_TIMEOUT_SECS: u64 =
    crate::mcp::stdio::CALL_TIMEOUT.as_secs() + MCP_TOOL_TIMEOUT_MARGIN.as_secs();

const MCP_TOOL_TIMEOUT_MARGIN: Duration = Duration::from_secs(60);

const _: () = assert!(MCP_TOOL_TIMEOUT_SECS > crate::mcp::stdio::CALL_TIMEOUT.as_secs());

/// The command and arguments every adapter registers as this session's MCP server. The session's
/// identity travels in argv: Claude Code and Grok do export a session id to an MCP child, but
/// Codex exports nothing and Grok's `{{session_id}}` templating does not work, so argv is the one
/// route all three agree on.
pub fn mcp_server_command(spec: &LaunchSpec<'_>) -> (String, Vec<String>) {
    let role = match spec.role {
        Role::Console => "console",
        Role::Project => "project",
    };
    let mut args = vec![
        "mcp".to_string(),
        "--session".to_string(),
        spec.session_id.to_string(),
        "--role".to_string(),
        role.to_string(),
        "--port".to_string(),
        spec.daemon_port.to_string(),
        "--token".to_string(),
        spec.mcp_token.to_string(),
    ];
    if spec.owner.is_some() {
        args.push("--bound".to_string());
    }
    (spec.self_exe.to_string(), args)
}

/// The session's pinned configuration directory, refused when it has gone and this launch is
/// resuming a conversation the agent actually has — `spec.resume_agent_session_id` is set only
/// then (see `protocol::Session::has_conversation` and `coordinator::resume_session`), never for a
/// fresh launch or for resuming a session that never had a turn. Those two launch into the missing
/// directory instead: Claude Code creates it itself, as it would for an unpinned launch, and the
/// Codex adapter creates it before launching.
///
/// Checked here rather than left to the agent: Claude Code would quietly create the missing
/// directory and come up logged out, without the conversation the resume was meant to continue —
/// an outcome that looks like success — and Codex would exit.
pub fn pinned_config_dir<'a>(spec: &LaunchSpec<'a>, agent: Agent) -> Result<Option<&'a Path>> {
    match spec.config_dir {
        Some(dir) if !dir.is_dir() && spec.resume_agent_session_id.is_some() => {
            Err(CodedError::raised(
                error_code::CONFIG_DIR_UNREACHABLE,
                format!(
                    "the {} config directory `{}` is not a directory the daemon can reach; \
                     recreate it, or point the account at a different directory",
                    agent.label(),
                    dir.display()
                ),
                &[("agent", agent.label()), ("path", &dir.to_string_lossy())],
            ))
        }
        dir => Ok(dir),
    }
}

/// Refuses an account directory this agent cannot launch against, ahead of anything being done in
/// it: the same rule `plan` applies at launch, asked beforehand so a switch can refuse before it
/// copies a conversation into the directory. The copy would otherwise create the directory's
/// `sessions/` and so make a Grok home that was never initialized pass the check at launch, which
/// is exactly the silent failure that check exists to prevent. `pinned` is the account's own
/// directory; the default account pins nothing and is not checked.
pub fn refuse_unlaunchable_account(agent: Agent, pinned: Option<&Path>) -> Result<()> {
    match (agent, pinned) {
        (Agent::Grok, Some(dir)) => grok::require_initialized_grok_home(dir),
        _ => Ok(()),
    }
}

/// The environment variable this agent reads its configuration directory from — the one an
/// adapter exports for a pinned account and a snapshot may already carry for the default one.
pub fn config_dir_var(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "CLAUDE_CONFIG_DIR",
        Agent::Codex => "CODEX_HOME",
        Agent::Grok => "GROK_HOME",
    }
}

/// The agent's own usual configuration directory under the home directory.
fn default_dir_name(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => ".claude",
        Agent::Codex => ".codex",
        Agent::Grok => ".grok",
    }
}

/// The directory this agent's default account resolves to — what "pinning nothing" means at
/// launch: the directory its own variable is exported to in `shell_env`, else the agent's own
/// usual default under the home directory the snapshot carries, else the daemon's own. For
/// Grok Build this is the *source* home a session's per-session home is built from
/// (`grok::build_grok_home`), never the per-session home itself, which is Octoboard's own and
/// exists only for the duration of a process.
///
/// Shared by `crate::availability`, which shows it as the default account's directory before any
/// session has opened, and by `grok::default_grok_home`, which a launch with no pinned account
/// builds the farm from.
pub fn default_account_dir(agent: Agent, shell_env: &HashMap<String, String>) -> PathBuf {
    let (var, default_name) = (config_dir_var(agent), default_dir_name(agent));
    if let Some(dir) = shell_env.get(var).filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = shell_env
        .get("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(crate::paths::home_dir);
    home.join(default_name)
}

/// A command running `agent`'s binary with `args` the way a session of the account would run it,
/// for a short query of the CLI rather than a launch: the binary resolved on `shell_env`'s
/// `PATH`, the environment the snapshot's and no more, the account's directory exported under
/// the agent's own variable when `config_dir` names one (left as the shell has it for the
/// default account), and the home directory as working directory, so no project-level
/// configuration of whatever directory the daemon started in takes part. `Err` carries the
/// binary's name when it does not resolve.
///
/// A caller running it outside a session or its launch claim holds
/// [`crate::shared_config::reading_account`] for the whole run: otherwise a change to the account's
/// sharing can move what the CLI opens, such as Codex's state database and history, from under it.
pub fn account_command(
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
) -> Result<std::process::Command, &'static str> {
    let binary = resolve_account_binary(agent, shell_env)?;
    Ok(account_command_of(
        &binary, agent, shell_env, config_dir, args,
    ))
}

/// Where `agent`'s binary resolves on `shell_env`'s `PATH`; `Err` carries the binary's name.
pub fn resolve_account_binary(
    agent: Agent,
    shell_env: &HashMap<String, String>,
) -> Result<String, &'static str> {
    let binary_name = adapter_for(agent).binary();
    crate::env_shell::resolve_binary(binary_name, shell_env).map_err(|_| binary_name)
}

/// [`account_command`] over a `binary` already resolved by [`resolve_account_binary`], for a caller
/// that makes several commands and wants them all to run the one it found.
pub fn account_command_of(
    binary: &str,
    agent: Agent,
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    args: &[&str],
) -> std::process::Command {
    let mut command = std::process::Command::new(binary);
    command.args(args).env_clear().envs(shell_env);
    if let Some(dir) = config_dir {
        command.env(config_dir_var(agent), dir);
    }
    command.current_dir(crate::paths::home_dir());
    command
}

#[derive(Default)]
pub struct LaunchPlan {
    pub args: Vec<String>,
    /// Environment entries layered on top of the shell snapshot.
    pub env: Vec<(String, String)>,
    /// Codex's user configuration or account override can resolve permissions automatically, so
    /// a permission hook alone does not establish a user interaction. Visible action prompts are
    /// observed separately.
    pub resolves_approvals_itself: bool,
    /// Something about this launch the user has to be told, because the agent will not tell them
    /// in a way they can act on. Surfaced once, when the session starts.
    pub notice: Option<Notice>,
    /// Where the agent records the trust its confirmation grants when that record cannot reach the
    /// user's own configuration by itself, and so has to be carried there once the confirmation is
    /// accepted, by Octoboard's press or in the terminal. Only Grok, whose trust store lives in the
    /// per-session home.
    pub carried_trust: Option<crate::trust::CarriedTrust>,
    /// The agent's configuration switches off what tells the trust watch that the agent is past its
    /// startup (Codex's terminal title), or cannot be read to say it does not; the watch is then
    /// bounded instead (`crate::trust::TrustState::new`).
    pub start_signal_missing: bool,
    /// Grok's: keys to copy from the session's `config.toml` onto the shared file once the process
    /// has exited. Only when that source file is a symlink.
    pub shared_config_writeback: Option<grok::SharedConfigWriteback>,
}

/// A title an agent's own record gives its conversation. Of the titles one record gives, the
/// highest `precedence` wins and the last of equal precedence wins.
pub struct NamedTitle {
    pub text: String,
    pub precedence: u8,
}

/// Whether `text` is nothing but a spinner's glyphs, the braille patterns agents animate.
pub fn is_spinner(text: &str) -> bool {
    let mut glyphs = text.chars().filter(|c| !c.is_whitespace()).peekable();
    glyphs.peek().is_some() && glyphs.all(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
}

pub trait AgentAdapter {
    /// Binary name, resolved against the snapshotted shell `PATH`.
    fn binary(&self) -> &'static str;

    /// True when the agent accepts an id for a new session, which lets Octoboard use its own
    /// session id as the agent's. Codex cannot, and its id only becomes known from the first hook
    /// payload.
    fn preallocates_session_id(&self) -> bool;

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan>;

    /// True when the plan runs the per-session hook script ([`write_hook_script`]) for its hooks,
    /// so a launch writes one only for such an agent.
    fn uses_hook_script(&self) -> bool {
        false
    }

    /// The version of the resolved `binary`, read from where it is installed instead of by running
    /// it: a process start is exactly what the caller is trying to avoid, and on a machine whose
    /// endpoint security stalls every exec a `--version` probe costs seconds. `None` when this
    /// agent's launch does not depend on its version, or when the install does not tell it.
    fn installed_version(&self, _binary: &str) -> Option<AgentVersion> {
        None
    }

    /// The task title a raw terminal title carries, or `None` when it carries none: agents decorate
    /// their titles with status glyphs, activity and their own or the project's name.
    /// `project_names` are the names the project may appear under, given only to an agent that
    /// `names_project`. An agent nobody has taught the shape of gets the title as sent.
    fn task_title(&self, raw: &str, _project_names: &[String]) -> Option<String> {
        let title = raw.trim();
        (!title.is_empty()).then(|| title.to_owned())
    }

    /// Whether the agent's terminal title carries the project's name, which `task_title` has to
    /// know to take it off.
    fn names_project(&self) -> bool {
        false
    }

    /// The title one line of the conversation's own append-only record names it with, if it is
    /// such a line.
    fn named_title(&self, _line: &str) -> Option<NamedTitle> {
        None
    }

    /// The conversation and its title named by one line of a file shared by all of an account's
    /// conversations, if it is such a line.
    fn indexed_title(&self, _line: &str) -> Option<(String, String)> {
        None
    }

    /// The prompt the user typed, when one line of the conversation's own record is it. Only for an
    /// agent that offers no title of its own to fall back on.
    fn prompt_text(&self, _line: &str) -> Option<String> {
        None
    }

    /// The title a conversation's summary file names it with.
    fn summary_title(&self, _summary: &serde_json::Value) -> Option<String> {
        None
    }
}

pub fn adapter_for(agent: Agent) -> Box<dyn AgentAdapter> {
    match agent {
        Agent::Claude => Box::new(claude::ClaudeAdapter),
        Agent::Codex => Box::new(codex::CodexAdapter),
        Agent::Grok => Box::new(grok::GrokAdapter),
    }
}

/// Hook timeout given to every agent, in seconds. Short on purpose: Grok defaults `Stop`,
/// `SubagentStop` and `PostToolUse` to 600 s because it treats them as gates, which would stall the
/// UI for ten minutes on a wedged hook, and Codex hard-clamps `SessionEnd` and `Interrupt` to 3 s
/// and prints a visible warning for anything above it.
pub const HOOK_TIMEOUT_SECS: u32 = 3;

/// An agent's version as major, minor, patch.
pub type AgentVersion = (u32, u32, u32);

/// Where an agent that posts its hooks itself sends them: the daemon's own hook route, which
/// takes the same body the forwarder would have posted.
pub fn hook_url(session_id: &str, daemon_port: u16) -> String {
    format!("http://127.0.0.1:{daemon_port}/hook/{session_id}")
}

/// The shell command that forwards one hook payload to the daemon: the body of the hook script,
/// and for an agent that runs its hook command through a shell, the whole hook, so the script's
/// own `/bin/sh` is not started on top of it. `exec` rather than a plain call so no shell lingers,
/// and stderr is dropped at the command level as well as inside the binary: a single stderr line
/// is enough to render an error cell in every one of the three agents.
pub fn hook_command(self_exe: &str, session_id: &str, daemon_port: u16) -> String {
    format!(
        "exec {} hook --session {} --port {} 2>/dev/null",
        shell_quote(self_exe),
        shell_quote(session_id),
        daemon_port
    )
}

/// Writes the per-session hook script and returns its path. One script per session rather than a
/// shared one, because it carries the session's identity and the daemon's port; it is removed with
/// the session's scratch directory.
///
/// The script execs the daemon binary in `hook` mode, which reads the payload, derives the event
/// from it, posts it to the daemon under a hard deadline, and exits 0 whatever happens.
pub fn write_hook_script(
    scratch: &Path,
    self_exe: &str,
    session_id: &str,
    daemon_port: u16,
) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let path = hook_script_path(scratch);
    let mut file = std::fs::File::create(&path)?;
    writeln!(file, "#!/bin/sh")?;
    writeln!(file, "{}", hook_command(self_exe, session_id, daemon_port))?;
    file.flush()?;
    let mut perms = file.metadata()?.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(&path, perms)?;
    Ok(path)
}

/// Where a session's hook script is, or would be, in its scratch directory.
pub fn hook_script_path(scratch: &Path) -> PathBuf {
    scratch.join("hook")
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::LaunchSpec;
    use crate::mcp::Owner;
    use crate::protocol::TerminalAppearance;

    /// A launch to plan against, with a real temporary directory standing in for the session's
    /// scratch space — the Grok adapter writes into it, so it cannot be a fiction.
    pub struct SpecFixture {
        pub session_id: String,
        pub role: crate::protocol::Role,
        pub owner: Option<Owner>,
        pub new_agent_session_id: String,
        pub cwd: PathBuf,
        pub scratch: crate::test_support::ScratchDir,
        pub hook_script: PathBuf,
        pub shell_env: HashMap<String, String>,
        pub config_dir: Option<PathBuf>,
        pub shared_default_dir: Option<PathBuf>,
        pub model: Option<String>,
        pub effort: Option<String>,
        pub terminal_appearance: Option<TerminalAppearance>,
        pub agent_version: Option<super::AgentVersion>,
        pub self_exe: String,
        pub mcp_token: String,
    }

    impl SpecFixture {
        pub fn spec(&self) -> LaunchSpec<'_> {
            LaunchSpec {
                session_id: &self.session_id,
                role: self.role,
                owner: self.owner.clone(),
                new_agent_session_id: &self.new_agent_session_id,
                resume_agent_session_id: None,
                cwd: &self.cwd,
                task: None,
                scratch: &self.scratch,
                hook_script: &self.hook_script,
                shell_env: &self.shell_env,
                config_dir: self.config_dir.as_deref(),
                approval_reviewer: None,
                approval_mode: None,
                shared_default_dir: self.shared_default_dir.as_deref(),
                model: self.model.as_deref(),
                effort: self.effort.as_deref(),
                terminal_appearance: self.terminal_appearance,
                agent_version: self.agent_version,
                self_exe: &self.self_exe,
                daemon_port: 4321,
                mcp_token: &self.mcp_token,
            }
        }
    }

    pub fn spec_fixture() -> SpecFixture {
        let root = crate::test_support::ScratchDir::new("adapter-test");
        let scratch = root.join("scratch");
        let cwd = root.join("project");
        std::fs::create_dir_all(&scratch).expect("scratch directory");
        std::fs::create_dir_all(&cwd).expect("project directory");
        let hook_script = scratch.join("hook");
        std::fs::write(&hook_script, "#!/bin/sh\nexit 0\n").expect("hook script");

        SpecFixture {
            session_id: "session-1".to_string(),
            role: crate::protocol::Role::Project,
            owner: Some(Owner::Console),
            new_agent_session_id: format!("agent-{}", std::process::id()),
            cwd,
            scratch: root,
            hook_script,
            shell_env: HashMap::new(),
            config_dir: None,
            shared_default_dir: None,
            model: None,
            effort: None,
            terminal_appearance: None,
            agent_version: None,
            self_exe: "/opt/octoboard/octoboardd".to_string(),
            mcp_token: "token-1".to_string(),
        }
    }

    /// The child starts its catalogue from its own arguments, so a bound session has to say so
    /// there and an unbound one must not; after launch it follows the daemon's answers.
    #[test]
    fn the_mcp_child_is_told_whether_the_session_is_bound() {
        let mut fixture = spec_fixture();
        let (_, bound) = super::mcp_server_command(&fixture.spec());
        assert!(bound.contains(&"--bound".to_string()));
        fixture.owner = None;
        let (_, unbound) = super::mcp_server_command(&fixture.spec());
        assert!(!unbound.contains(&"--bound".to_string()));
    }
}
