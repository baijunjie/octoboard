//! Grok Build adapter. A6/A7 settled Grok's injection mechanism:
//! - **A6** (hooks + MCP): the obvious config-directory overrides do not work —
//!   `GROK_CONFIG`/`GROK_CONFIG_PATH` layer on top of `config.toml` for an allowlist of keys that
//!   silently drops `hooks` and `mcp_servers`. What works is pointing `GROK_HOME` at an
//!   Octoboard-owned directory built as a symlink farm over the user's real `~/.grok` — every
//!   entry a symlink except `config.toml` (copied from the user's, with an
//!   `[mcp_servers.octoboard]` block appended) and `hooks/` (Octoboard's own hooks). `auth.json`
//!   and `sessions/` stay symlinks so login state and resumability are shared with the user's own
//!   plain `grok`.
//! - **A7** (role description): `--rules <text>` appends to the system prompt without replacing
//!   it, so no smuggling into the initial task is needed.
//!
//! See the technical-validation plan's A6/A7 entries for the verified detail and the conditions
//! (project trust, git workspace root) that silently disable the project's *own* configuration if
//! missed.
//!
//! This adapter does not implement the `GROK_HOME` mechanism yet — it only wires up `--rules`
//! and otherwise launches in the documented degraded "ordinary terminal session" mode: no status
//! hooks, no MCP tools.

use super::{role_description, AgentAdapter, LaunchContext};

/// Placeholder for the `GROK_HOME` symlink-farm mechanism A6 settled on. `None` is the only
/// variant implemented right now — Grok launches without hooks or MCP, which is the explicitly
/// documented degraded mode, not a bug.
pub enum GrokInjection {
    None,
    // TODO(milestone 01): add a variant that builds the per-session `GROK_HOME` symlink farm
    // (A6) and wire it into `build_args` below.
}

pub struct GrokAdapter {
    pub injection: GrokInjection,
}

impl AgentAdapter for GrokAdapter {
    fn binary(&self) -> &str {
        "grok"
    }

    fn build_args(&self, ctx: &LaunchContext) -> Vec<String> {
        let mut args = Vec::new();

        match &ctx.resume_agent_session_id {
            // The id is required: a bare `--resume` swallows the next argument as its value, which
            // here would be the role description.
            Some(id) => {
                args.push("--resume".to_string());
                args.push(id.clone());
            }
            None => {
                args.push("--session-id".to_string());
                args.push(ctx.session_id.to_string());
            }
        }

        args.push("--rules".to_string());
        args.push(role_description(ctx.role));

        if ctx.resume_agent_session_id.is_none() {
            if let Some(task) = &ctx.task {
                args.push(task.clone());
            }
        }

        match self.injection {
            GrokInjection::None => {
                // TODO(milestone 01): inject hooks + MCP via the `GROK_HOME` mechanism (A6).
                // Until then Grok sessions never report status and expose no MCP tools.
            }
        }

        args
    }
}
