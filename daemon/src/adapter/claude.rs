//! Claude Code adapter. Injection is by flags only, and `--settings` takes a JSON string whose
//! `hooks` merge with the project's `.claude/settings.json` rather than replacing them.
//!
//! Two flags must never be passed, both of which silently drop the *project's* own configuration —
//! the exact failure Octoboard's "project configuration must not be overridden" rule exists to
//! prevent: `--setting-sources` (drops the project's permission rules and its hooks) and
//! `--strict-mcp-config` (drops the project's and the user's MCP servers). The injected MCP
//! server's key must not collide with one the project defines either, or the project's own
//! definition is silently never spawned.
//!
//! In a workspace the user has not trusted, Claude Code ignores the project's own `allow` rules
//! while still applying its `deny` rules, so the session is only ever more restrictive and nothing
//! breaks — but the user has no way to learn why their project's permissions are not applying. It
//! says so on stderr, which on a PTY is the same stream as the rendered UI, so the trust state is
//! read out of `~/.claude.json` instead of matched in terminal text. Read, never written: a trust
//! decision is the user's to make in Claude Code itself.

use serde_json::json;

use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};
use crate::mcp;
use crate::protocol::Agent;

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

    /// The `--mcp-config` JSON. Merged with the project's and the user's servers, which is why
    /// `--strict-mcp-config` must never be passed alongside it.
    fn mcp_config_json(spec: &LaunchSpec<'_>) -> String {
        let (command, args) = super::mcp_server_command(spec);
        json!({
            "mcpServers": {
                mcp::SERVER_KEY: {
                    "type": "stdio",
                    "command": command,
                    "args": args,
                }
            }
        })
        .to_string()
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

        // The task goes first, ahead of every flag. `--mcp-config` takes a *list* of values, so
        // anything non-flag after it is read as another config path — `claude --mcp-config <json>
        // mcp list` fails with "MCP config file not found: …/mcp" — and a task left at the end
        // would be swallowed the same way by it or by any other list-valued flag.
        if let Some(task) = spec.task {
            args.push(task.to_string());
        }

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
        args.push("--mcp-config".to_string());
        args.push(Self::mcp_config_json(spec));
        args.push("--append-system-prompt".to_string());
        args.push(mcp::role::role_description(spec.role, Agent::Claude));

        Ok(LaunchPlan {
            args,
            notice: untrusted_workspace_notice(spec),
            ..LaunchPlan::default()
        })
    }
}

/// What to tell the user when Claude Code will ignore this project's own `allow` rules.
///
/// Deliberately silent unless the trust state is explicitly negative: the field is only read, so
/// an absent project entry or a renamed key must leave the user alone rather than warn them on
/// every launch about something that may not be true.
fn untrusted_workspace_notice(spec: &LaunchSpec<'_>) -> Option<String> {
    let home = spec
        .shell_env
        .get("HOME")
        .filter(|home| !home.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(crate::paths::home_dir);
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join(".claude.json")).ok()?).ok()?;

    let canonical = std::fs::canonicalize(spec.cwd).unwrap_or_else(|_| spec.cwd.to_path_buf());
    let project = config
        .get("projects")?
        .get(canonical.to_string_lossy().as_ref())?;
    match project.get("hasTrustDialogAccepted") {
        Some(serde_json::Value::Bool(false)) => Some(
            "Claude Code has not been trusted with this directory, so this project's own `allow` \
             permission rules are ignored for this session — its `deny` rules still apply, so the \
             session is only more restrictive, never less. Accept the trust prompt in Claude Code \
             itself to change that; Octoboard does not write that decision for you."
                .to_string(),
        ),
        _ => None,
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

    /// Reading the trust state wrong must not nag: only an explicit `false` warns, so an absent
    /// project entry or a key that was renamed upstream leaves the user alone.
    #[test]
    fn only_an_explicitly_untrusted_workspace_produces_a_notice() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("home");
        std::fs::create_dir_all(&home).expect("home");
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        let canonical = std::fs::canonicalize(&fixture.cwd).expect("canonical path");
        let config = home.join(".claude.json");

        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_none());

        std::fs::write(&config, r#"{"projects":{}}"#).expect("config");
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_none());

        let untrusted = serde_json::json!({
            "projects": { canonical.to_string_lossy(): { "hasTrustDialogAccepted": false } }
        });
        std::fs::write(&config, untrusted.to_string()).expect("config");
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_some());

        let trusted = serde_json::json!({
            "projects": { canonical.to_string_lossy(): { "hasTrustDialogAccepted": true } }
        });
        std::fs::write(&config, trusted.to_string()).expect("config");
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_none());
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

    /// `--mcp-config` takes a list of values, so a task left after the flags is read as another
    /// config path and never reaches the model.
    #[test]
    fn the_task_comes_before_every_flag() {
        let fixture = spec_fixture();
        let mut spec = fixture.spec();
        spec.task = Some("do the thing");
        let plan = ClaudeAdapter.plan(&spec).expect("plan");
        assert_eq!(plan.args[0], "do the thing");
        assert!(plan.args[1].starts_with("--"));
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
        assert!(resumed.args.iter().any(|arg| arg == "--mcp-config"));
        // Passed on every launch even though the first one is what Claude Code records: after a
        // compaction the snapshot is re-rendered from whatever that launch passed.
        assert!(resumed
            .args
            .iter()
            .any(|arg| arg == "--append-system-prompt"));
    }

    #[test]
    fn registers_the_mcp_server_as_a_stdio_child_under_octoboards_own_key() {
        let fixture = spec_fixture();
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        let at = plan
            .args
            .iter()
            .position(|arg| arg == "--mcp-config")
            .expect("--mcp-config is passed");
        let config: serde_json::Value =
            serde_json::from_str(&plan.args[at + 1]).expect("the MCP config is JSON");
        let server = &config["mcpServers"][mcp::SERVER_KEY];
        assert_eq!(server["type"], "stdio");
        assert_eq!(server["command"], fixture.self_exe);
        let args: Vec<String> = serde_json::from_value(server["args"].clone()).expect("args");
        assert_eq!(args[0], "mcp");
        assert!(args.contains(&fixture.session_id));
        assert!(args.contains(&fixture.mcp_token));
    }

    #[test]
    fn the_role_text_is_the_one_for_this_sessions_role() {
        let fixture = spec_fixture();
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        let at = plan
            .args
            .iter()
            .position(|arg| arg == "--append-system-prompt")
            .expect("--append-system-prompt is passed");
        assert_eq!(
            plan.args[at + 1],
            mcp::role::role_description(fixture.role, Agent::Claude)
        );
    }
}
