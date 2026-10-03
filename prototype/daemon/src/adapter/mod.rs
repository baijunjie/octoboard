//! Agent adapter trait: one implementation per agent CLI, each responsible for assembling the
//! launch argv that injects Octoboard's hooks / MCP server / role description for that agent.
//! The rest of the daemon (PTY management, the WebSocket server) is agent-agnostic.

pub mod claude;
pub mod codex;
pub mod grok;

use uuid::Uuid;

use crate::protocol::Role;

/// Everything an adapter needs to assemble one launch's argv. Shared across new launches and
/// `--resume`d ones (G6: injected arguments are reassembled on resume, not reused as-is).
pub struct LaunchContext {
    pub session_id: Uuid,
    pub role: Role,
    /// Initial task text, rendered as the agent's positional prompt argument.
    pub task: Option<String>,
    pub daemon_port: u16,
    /// Path to this same `obd-proto` binary, used to point `--mcp-config` at `obd-proto mcp ...`.
    pub self_exe: String,
    /// Set when relaunching an existing session rather than starting a fresh one.
    pub resume_agent_session_id: Option<String>,
}

pub trait AgentAdapter: Send + Sync {
    /// Binary name to resolve via PATH (from the snapshotted shell environment).
    fn binary(&self) -> &str;

    /// Full argv, not including argv[0] (the binary name itself).
    fn build_args(&self, ctx: &LaunchContext) -> Vec<String>;
}

/// Role description text injected into the agent's system prompt. Shared between adapters;
/// kept here because it is about Octoboard's orchestration model, not about any one CLI.
pub fn role_description(role: Role) -> String {
    match role {
        Role::Hub => {
            "You are the Octoboard hub session. You coordinate work across projects via the \
             `start_session` and `show_page` MCP tools exposed by the `octoboard` server. You do \
             not edit project code yourself."
                .to_string()
        }
        Role::Worker => {
            "You are an Octoboard project worker session. Use the `report` MCP tool exposed by \
             the `octoboard` server to summarize your work back to the hub, and `ask_user` when \
             you need a decision only a person can make."
                .to_string()
        }
    }
}
