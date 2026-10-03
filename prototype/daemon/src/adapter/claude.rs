//! Claude Code adapter. The one adapter wired end to end in this prototype — every flag
//! used here was verified by hand on the target machine before this code was written:
//! `--session-id`, `--settings` taking a JSON *string* whose `hooks` merge with (rather than
//! override) the project's `.claude/settings.json` hooks, `--mcp-config` taking a JSON string,
//! and `--append-system-prompt`.

use serde_json::json;

use super::{role_description, AgentAdapter, LaunchContext};
use crate::protocol::Role;

/// Hook events wired up, mapped to the `status` states in PROTOCOL.md on the receiving end
/// (see `server::map_claude_hook_status`). G3 settled this mapping; see the technical-validation
/// plan's G3 entry for the verified detail and the gaps it accepts.
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    // A pending permission prompt. `Notification`'s `permission_prompt` type was never observed
    // firing, so this is the only event that sees one.
    "PermissionRequest",
    "Notification",
    "Stop",
    // Mutually exclusive with `Stop`: a turn ending in an API error fires only this one, so a
    // registry without it leaves the session looking busy forever.
    "StopFailure",
    "SessionEnd",
];

pub struct ClaudeAdapter;

impl ClaudeAdapter {
    /// The `--settings` JSON string: one hook command per event, shaped to G4's required form —
    /// the fail-fast-and-silent `curl -s -m 2 -o /dev/null ... || true` so a dead daemon never
    /// hangs or errors out the agent, plus `"async": true` and an explicit short `"timeout"` so
    /// a slow hook never sits on the turn's critical path (G4 measured a 30 s hook taking a turn
    /// from 6.8 s to 36.7 s with neither of those set).
    fn settings_json(ctx: &LaunchContext) -> String {
        let mut hooks = serde_json::Map::new();
        for event in HOOK_EVENTS {
            let url = format!(
                "http://127.0.0.1:{}/hook/{}/{}",
                ctx.daemon_port, ctx.session_id, event
            );
            let command =
                format!("curl -s -m 2 -o /dev/null -X POST --data-binary @- {url} || true");
            hooks.insert(
                event.to_string(),
                json!([{
                    "hooks": [{
                        "type": "command",
                        "command": command,
                        "async": true,
                        "timeout": 2,
                    }]
                }]),
            );
        }
        json!({ "hooks": hooks }).to_string()
    }

    /// The `--mcp-config` JSON string: a single stdio server, `obd-proto` itself in `mcp` mode,
    /// carrying the session id and role in argv so the stdio server (and the daemon behind it)
    /// knows whose tool set to expose (G5).
    fn mcp_config_json(ctx: &LaunchContext) -> String {
        let role = match ctx.role {
            Role::Hub => "hub",
            Role::Worker => "worker",
        };
        json!({
            "mcpServers": {
                "octoboard": {
                    "command": ctx.self_exe,
                    "args": [
                        "mcp",
                        "--session", ctx.session_id.to_string(),
                        "--role", role,
                        "--daemon", format!("http://127.0.0.1:{}", ctx.daemon_port),
                    ]
                }
            }
        })
        .to_string()
    }
}

impl AgentAdapter for ClaudeAdapter {
    fn binary(&self) -> &str {
        "claude"
    }

    fn build_args(&self, ctx: &LaunchContext) -> Vec<String> {
        let mut args = Vec::new();

        match &ctx.resume_agent_session_id {
            Some(id) => {
                args.push("--resume".to_string());
                args.push(id.clone());
            }
            None => {
                args.push("--session-id".to_string());
                args.push(ctx.session_id.to_string());
            }
        }

        args.push("--settings".to_string());
        args.push(Self::settings_json(ctx));
        args.push("--mcp-config".to_string());
        args.push(Self::mcp_config_json(ctx));
        args.push("--append-system-prompt".to_string());
        args.push(role_description(ctx.role));

        // The initial task is the positional prompt; on resume there is nothing to add unless
        // the hub sends a fresh instruction, which goes through `send_message` instead.
        if let Some(task) = &ctx.task {
            args.push(task.clone());
        }

        args
    }
}
