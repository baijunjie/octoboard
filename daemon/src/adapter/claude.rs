//! Claude Code adapter. Injection is by flags only, and `--settings` takes a JSON string whose
//! `hooks` merge with the project's `.claude/settings.json` rather than replacing them.
//!
//! Two flags must never be passed, both of which silently drop the *project's* own configuration —
//! the exact failure Octoboard's "project configuration must not be overridden" rule exists to
//! prevent: `--setting-sources` (drops the project's permission rules and its hooks) and
//! `--strict-mcp-config` (drops the project's and the user's MCP servers).

use serde_json::json;

use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};

/// The events that carry the session states Octoboard shows. `StopFailure` is registered next to
/// `Stop` because the two are mutually exclusive — a turn ending in an API error fires only
/// `StopFailure`, and a daemon keying "finished" on `Stop` alone leaves that session looking busy
/// forever. `PostToolBatch` is here because a tool call rejected by Claude Code's own
/// pre-execution guard fires only that one, skipping `PreToolUse` and both post events.
const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PostToolBatch",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
];

// TODO(milestone 02): surface the untrusted-workspace warning. In a workspace the user has not
// trusted, Claude Code ignores the project's own `allow` rules and says so on stderr — the session
// is only ever more restrictive, so nothing breaks silently, but the user has no way to know why
// their project's permissions are not applying. Trust lives in `~/.claude.json`, which Octoboard
// must not write, so the only route is to surface the line. It needs a channel that does not exist
// yet: the agent runs on a PTY, where stderr is the same stream as the rendered UI, so the daemon
// would have to match it in the terminal output rather than read a separate descriptor.
pub struct ClaudeAdapter;

impl ClaudeAdapter {
    /// The `--settings` JSON. `async` keeps a slow hook off the turn's critical path (a 30 s hook
    /// took a turn from 6.8 s to 36.7 s without it) and `timeout` bounds it regardless.
    fn settings_json(spec: &LaunchSpec<'_>) -> String {
        let command = spec.hook_script.to_string_lossy().into_owned();
        let mut hooks = serde_json::Map::new();
        for event in HOOK_EVENTS {
            hooks.insert(
                (*event).to_string(),
                json!([{
                    "hooks": [{
                        "type": "command",
                        "command": command,
                        "async": true,
                        "timeout": HOOK_TIMEOUT_SECS,
                    }]
                }]),
            );
        }
        json!({ "hooks": hooks }).to_string()
    }
}

impl AgentAdapter for ClaudeAdapter {
    fn binary(&self) -> &'static str {
        "claude"
    }

    fn preallocates_session_id(&self) -> bool {
        true
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> anyhow::Result<LaunchPlan> {
        let mut args = Vec::new();
        match spec.resume_agent_session_id {
            Some(id) => {
                args.push("--resume".to_string());
                args.push(id.to_string());
            }
            None => {
                args.push("--session-id".to_string());
                args.push(spec.new_agent_session_id.to_string());
            }
        }
        args.push("--settings".to_string());
        args.push(Self::settings_json(spec));

        if let Some(task) = spec.task {
            args.push(task.to_string());
        }

        Ok(LaunchPlan {
            args,
            env: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;

    #[test]
    fn never_passes_the_flags_that_drop_the_projects_own_configuration() {
        let fixture = spec_fixture();
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        for forbidden in ["--setting-sources", "--strict-mcp-config"] {
            assert!(
                !plan.args.iter().any(|arg| arg == forbidden),
                "{forbidden} must never be passed"
            );
        }
    }

    #[test]
    fn injects_every_hook_event_as_a_fast_silent_command() {
        let fixture = spec_fixture();
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        let settings_index = plan
            .args
            .iter()
            .position(|arg| arg == "--settings")
            .expect("--settings is passed");
        let settings: serde_json::Value =
            serde_json::from_str(&plan.args[settings_index + 1]).expect("the settings are JSON");
        let hooks = settings["hooks"].as_object().expect("a hooks table");
        assert_eq!(hooks.len(), HOOK_EVENTS.len());
        for event in HOOK_EVENTS {
            let handler = &hooks[*event][0]["hooks"][0];
            assert_eq!(handler["type"], "command");
            assert_eq!(handler["async"], true);
            assert_eq!(handler["timeout"], HOOK_TIMEOUT_SECS);
            assert_eq!(
                handler["command"].as_str().unwrap(),
                fixture.hook_script.to_string_lossy()
            );
        }
    }

    #[test]
    fn a_new_session_takes_octoboards_id_and_a_resume_takes_the_agents() {
        let fixture = spec_fixture();
        let fresh = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        assert_eq!(fresh.args[0], "--session-id");
        assert_eq!(fresh.args[1], fixture.new_agent_session_id);

        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some("agent-side-id");
        let resumed = ClaudeAdapter.plan(&spec).expect("plan");
        assert_eq!(resumed.args[0], "--resume");
        assert_eq!(resumed.args[1], "agent-side-id");
        // The injection is reassembled on a resume too: without it the resumed session fires no
        // hooks and nobody observes it.
        assert!(resumed.args.iter().any(|arg| arg == "--settings"));
    }
}
