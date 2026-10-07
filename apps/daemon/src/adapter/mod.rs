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
//!   record. The role text is therefore a function of the session's role and agent alone, and a
//!   session's role is immutable for its lifetime. It is still passed on every launch, because
//!   Claude Code re-renders its snapshot from whatever *that* launch passed after a compaction.
//! - **Hooks must fail fast and silently.** Every agent surfaces a failing hook to the user, and a
//!   hook with no timeout blocks the turn for its full duration. Octoboard owns the hook script
//!   (`hook_script` below, generated per session), which exits 0 unconditionally and writes
//!   nothing.
//!
//! The hook script itself takes no arguments: it derives the event from the payload on stdin, so
//! one constant command serves every event, and adding or removing an event never changes what an
//! adapter has to generate beyond the event name itself.

pub mod claude;
pub mod codex;
pub mod grok;

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::protocol::{error_code, Agent, CodedError, Notice, Role};

/// Everything an adapter needs to assemble one launch.
pub struct LaunchSpec<'a> {
    /// Octoboard's own session id. Not the agent's — it identifies the session to the daemon, and
    /// is what the injected MCP server is launched with.
    pub session_id: &'a str,
    /// Which side of the orchestration this session is on, which decides both the role text and
    /// the tools its MCP server announces.
    pub role: Role,
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
    /// Absolute path of the generated hook script for this session.
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
    /// The daemon binary, which is also the MCP server and the hook forwarder.
    pub self_exe: &'a str,
    /// Where the daemon is listening, for the MCP server this launch registers.
    pub daemon_port: u16,
    /// The token that session's MCP server authenticates to the daemon with.
    pub mcp_token: &'a str,
}

/// The command and arguments every adapter registers as this session's MCP server. The session's
/// identity travels in argv: Claude Code and Grok do export a session id to an MCP child, but
/// Codex exports nothing and Grok's `{{session_id}}` templating does not work, so argv is the one
/// route all three agree on.
pub fn mcp_server_command(spec: &LaunchSpec<'_>) -> (String, Vec<String>) {
    let role = match spec.role {
        Role::Console => "console",
        Role::Project => "project",
    };
    (
        spec.self_exe.to_string(),
        vec![
            "mcp".to_string(),
            "--session".to_string(),
            spec.session_id.to_string(),
            "--role".to_string(),
            role.to_string(),
            "--port".to_string(),
            spec.daemon_port.to_string(),
            "--token".to_string(),
            spec.mcp_token.to_string(),
        ],
    )
}

/// The session's pinned configuration directory, refused when it has gone. Checked here rather than
/// left to the agent, which would quietly create the missing directory and start the session
/// logged out, without the conversation it is meant to resume.
pub fn pinned_config_dir<'a>(spec: &LaunchSpec<'a>, agent: Agent) -> Result<Option<&'a Path>> {
    match spec.config_dir {
        Some(dir) if !dir.is_dir() => Err(CodedError::raised(
            error_code::CONFIG_DIR_UNREACHABLE,
            format!(
                "the {} config directory `{}` is not a directory the daemon can reach; \
                 recreate it, or clear it in the console's settings so new sessions \
                 start without it",
                agent.label(),
                dir.display()
            ),
            &[("agent", agent.label()), ("path", &dir.to_string_lossy())],
        )),
        dir => Ok(dir),
    }
}

#[derive(Default)]
pub struct LaunchPlan {
    pub args: Vec<String>,
    /// Environment entries layered on top of the shell snapshot.
    pub env: Vec<(String, String)>,
    /// The agent resolves approval requests itself for this session, so its permission hook fires
    /// without any dialog ever reaching the user. Octoboard must not raise a hand for one: nobody
    /// would have anything to answer, and the tool proceeds regardless. Only Codex can be in this
    /// state, and only because of the user's own configuration.
    pub resolves_approvals_itself: bool,
    /// Something about this launch the user has to be told, because the agent will not tell them
    /// in a way they can act on. Surfaced once, when the session starts.
    pub notice: Option<Notice>,
}

pub trait AgentAdapter {
    /// Binary name, resolved against the snapshotted shell `PATH`.
    fn binary(&self) -> &'static str;

    /// True when the agent accepts an id for a new session, which lets Octoboard use its own
    /// session id as the agent's. Codex cannot, and its id only becomes known from the first hook
    /// payload.
    fn preallocates_session_id(&self) -> bool;

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan>;
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
) -> Result<std::path::PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let path = scratch.join("hook");
    let mut file = std::fs::File::create(&path)?;
    writeln!(file, "#!/bin/sh")?;
    // `exec` rather than a plain call so no shell lingers, and stderr is dropped at the script
    // level as well as inside the binary: a single stderr line is enough to render an error cell
    // in every one of the three agents.
    writeln!(
        file,
        "exec {} hook --session {} --port {} 2>/dev/null",
        shell_quote(self_exe),
        shell_quote(session_id),
        daemon_port
    )?;
    file.flush()?;
    let mut perms = file.metadata()?.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(&path, perms)?;
    Ok(path)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::LaunchSpec;

    /// A launch to plan against, with a real temporary directory standing in for the session's
    /// scratch space — the Grok adapter writes into it, so it cannot be a fiction.
    pub struct SpecFixture {
        pub session_id: String,
        pub role: crate::protocol::Role,
        pub new_agent_session_id: String,
        pub cwd: PathBuf,
        pub scratch: PathBuf,
        pub hook_script: PathBuf,
        pub shell_env: HashMap<String, String>,
        pub config_dir: Option<PathBuf>,
        pub self_exe: String,
        pub mcp_token: String,
    }

    impl SpecFixture {
        pub fn spec(&self) -> LaunchSpec<'_> {
            LaunchSpec {
                session_id: &self.session_id,
                role: self.role,
                new_agent_session_id: &self.new_agent_session_id,
                resume_agent_session_id: None,
                cwd: &self.cwd,
                task: None,
                scratch: &self.scratch,
                hook_script: &self.hook_script,
                shell_env: &self.shell_env,
                config_dir: self.config_dir.as_deref(),
                self_exe: &self.self_exe,
                daemon_port: 4321,
                mcp_token: &self.mcp_token,
            }
        }
    }

    impl Drop for SpecFixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.scratch).ok();
        }
    }

    pub fn spec_fixture() -> SpecFixture {
        let root = std::env::temp_dir().join(format!(
            "octoboardd-adapter-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let scratch = root.join("scratch");
        let cwd = root.join("project");
        std::fs::create_dir_all(&scratch).expect("scratch directory");
        std::fs::create_dir_all(&cwd).expect("project directory");
        let hook_script = scratch.join("hook");
        std::fs::write(&hook_script, "#!/bin/sh\nexit 0\n").expect("hook script");

        SpecFixture {
            session_id: "session-1".to_string(),
            role: crate::protocol::Role::Project,
            new_agent_session_id: format!("agent-{}", std::process::id()),
            cwd,
            scratch: root,
            hook_script,
            shell_env: HashMap::new(),
            config_dir: None,
            self_exe: "/opt/octoboard/octoboardd".to_string(),
            mcp_token: "token-1".to_string(),
        }
    }
}
