//! Codex adapter. Added after the rest of this prototype's design was settled, once Codex
//! turned out to be installed on the machine after all (`~/.local/bin/codex`, which is why it
//! needs the login+interactive shell env snapshot just like `grok` does — `~/.local/bin` is a
//! `~/.zshrc` PATH addition). A2-A5 settled Codex's injection mechanism:
//! - **A2** (session id): no id can be pre-allocated — there is no `--session-id`-shaped flag,
//!   and setting `CODEX_SESSION_ID`/`CODEX_THREAD_ID` before launch is ignored. The reliable
//!   source is the `session_id` field of the `SessionStart` hook payload, which (in the
//!   interactive TUI) arrives lazily on the thread's first prompt submission, not at process
//!   start. Matching the session directory by cwd/start time — the plan's original fallback —
//!   turned out to be unnecessary.
//! - **A3** (hooks): injected via one `-c 'hooks.<Event>=[...]'` per event, each entry
//!   `type="command"`, `async=true`, `timeout<=3` (longer is hard-clamped on `SessionEnd`/
//!   `Interrupt` and prints a warning). Hooks must also get past Codex's hook-trust gate —
//!   `--dangerously-bypass-hook-trust`, or a seeded `-c 'hooks.state={...}'` to avoid its two
//!   warning lines — or `codex exec` hangs indefinitely with no hook firing and no error.
//!   **`CODEX_HOME` must not be used**: it makes Codex ignore the user's entire global setup
//!   (`AGENTS.md`, model choice, approval policy, skills, plugins), not just add to it.
//! - **A4** (role description): `-c 'developer_instructions="<text>"'` prepends a developer
//!   message without replacing the base system prompt.
//! - **A5** (MCP): `-c 'mcp_servers.<name>.command="..."'` plus
//!   `-c 'mcp_servers.<name>.args=[...]'` merges with the user's own servers.
//!
//! See the technical-validation plan's A2-A5 entries for the verified detail. Until this
//! adapter implements any of it, Codex launches and renders in a PTY with none of it wired up —
//! the same documented degraded mode as Grok.

use super::{AgentAdapter, LaunchContext};

/// Placeholder for the `-c`-based hook/MCP/role-prompt injection A2-A5 settled on. `None` is the
/// only variant implemented right now.
pub enum CodexInjection {
    None,
    // TODO(milestone 01): add a variant that passes the `-c hooks.*`, `-c mcp_servers.*` and
    // `-c developer_instructions=...` flags A2-A5 settled on, and wire it into `build_args`.
}

pub struct CodexAdapter {
    pub injection: CodexInjection,
}

impl AgentAdapter for CodexAdapter {
    fn binary(&self) -> &str {
        "codex"
    }

    fn build_args(&self, ctx: &LaunchContext) -> Vec<String> {
        let mut args = Vec::new();

        // Unlike claude/grok, Codex's session id cannot be pre-allocated at all (A2): this
        // adapter never passes a `--session-id`-shaped flag, so a fresh launch gets whatever id
        // Codex assigns on its own, discoverable only through the `SessionStart` hook payload
        // once A3's hook injection is implemented here. `resume_agent_session_id` therefore has
        // to be the real Codex session id recovered that way — this adapter does not yet produce
        // one.
        match &ctx.resume_agent_session_id {
            Some(id) => {
                args.push("resume".to_string());
                args.push(id.clone());
            }
            None => {
                if let Some(task) = &ctx.task {
                    args.push(task.clone());
                }
            }
        }

        match self.injection {
            CodexInjection::None => {
                // TODO(milestone 01): inject hooks + MCP + role description per A3-A5. Until
                // then Codex sessions never report status, expose no MCP tools, and get no
                // appended role description.
            }
        }

        args
    }
}
