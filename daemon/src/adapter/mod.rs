//! Agent adapters: one per agent CLI, each assembling the launch of that CLI so Octoboard's
//! capabilities are injected *additionally*, without modifying project files or the user's own
//! configuration. Everything else in the daemon is agent-agnostic.
//!
//! What is injected in this milestone is the status hooks. The injected MCP server and the role
//! description belong to the orchestration the hub performs, and the tools they would announce do
//! not exist yet.
//! TODO(milestone 02): add the MCP server and the role description to each adapter — the verified
//! per-agent mechanisms are in the "Agent adapters" table of `docs/mvp.md` section 6. Note that
//! Claude Code records an appended system prompt on a conversation's first request and replays it
//! verbatim afterwards, so a session started in this milestone can never be given a role later; 02
//! has to start a new session rather than resume one for that.
//!
//! Three rules hold for every adapter, each of which fails silently if broken:
//!
//! - **Resume re-injects everything.** For all three agents, hooks are resolved from the launch
//!   arguments every time and are lost on a resume that omits them, leaving a session nobody
//!   observes. Adapters therefore assemble the full injection on every launch, resume included.
//! - **Hooks must fail fast and silently.** Every agent surfaces a failing hook to the user, and a
//!   hook with no timeout blocks the turn for its full duration. Octoboard owns the hook script
//!   (`hook_script` below, generated per session), which exits 0 unconditionally and writes
//!   nothing.
//! - **The hook script takes no arguments**, and derives the event from the payload on stdin. Codex
//!   gates hooks behind a trust hash over the handler definition, so a per-event command string
//!   would make those hashes per-event as well; keeping one constant command leaves the door open
//!   to seeding them later instead of passing `--dangerously-bypass-hook-trust`.

pub mod claude;
pub mod codex;
pub mod grok;

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::protocol::Agent;

/// Everything an adapter needs to assemble one launch.
pub struct LaunchSpec<'a> {
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
}

pub struct LaunchPlan {
    pub args: Vec<String>,
    /// Environment entries layered on top of the shell snapshot.
    pub env: Vec<(String, String)>,
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
        pub new_agent_session_id: String,
        pub cwd: PathBuf,
        pub scratch: PathBuf,
        pub hook_script: PathBuf,
        pub shell_env: HashMap<String, String>,
    }

    impl SpecFixture {
        pub fn spec(&self) -> LaunchSpec<'_> {
            LaunchSpec {
                new_agent_session_id: &self.new_agent_session_id,
                resume_agent_session_id: None,
                cwd: &self.cwd,
                task: None,
                scratch: &self.scratch,
                hook_script: &self.hook_script,
                shell_env: &self.shell_env,
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
            new_agent_session_id: format!("agent-{}", std::process::id()),
            cwd,
            scratch: root,
            hook_script,
            shell_env: HashMap::new(),
        }
    }
}
