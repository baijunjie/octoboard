//! Codex adapter. Everything is injected through repeated `-c` overrides, which are
//! per-invocation and persisted nowhere — so the complete set has to be re-passed on every
//! `codex resume` too.
//!
//! `CODEX_HOME` must not be used to *inject* anything: it makes Codex ignore the user's entire
//! global setup (`~/.codex/AGENTS.md`, model choice, approval policy, skills, plugins), not just add
//! to it, which is the same objection as modifying their configuration. The one time it is set is
//! when the user has pointed a console at a Codex home of their own: that directory then *is* their
//! global setup, login and transcripts included, and a resume has to find it in the same place.
//!
//! The role description goes in `developer_instructions`, which adds a developer message and
//! leaves the rest of the prompt byte-identical. Not `instructions`, which *replaces* the system
//! prompt.
//!
//! Two conditions are launch prerequisites rather than polish:
//!
//! - **Project trust.** Without it the folder-trust modal appears. The table must be passed whole
//!   (`-c 'projects={"<path>"={trust_level="trusted"}}'`) because `-c` splits its key on `.` and
//!   silently ignores a dotted path containing a quoted segment, and the path has to be the
//!   canonical one — `/private/tmp/…`, not `/tmp/…`.
//! - **Hook trust.** Codex gates any new or changed hook behind a persisted trust hash. Without it
//!   an interactive session raises a blocking review modal and no hook runs, and a headless one
//!   hangs indefinitely with no output at all. `--dangerously-bypass-hook-trust` is the verified
//!   way through; despite the name it weakens neither the sandbox nor the approval policy, it only
//!   skips the review of hooks Octoboard generated itself. The cost — two warning lines on every
//!   launch — is permanent short of redesigning the injection so the hook command is
//!   session-independent: the hash is taken over the handler definition, which includes the hook
//!   command, and that command is the session's own hook script path, so seeding `hooks.state`
//!   with hashes shipped with the adapter cannot work — no hash captured once could ever match a
//!   different session's.
//!
//! One of the user's own settings changes what Octoboard may promise: with `approvals_reviewer`
//! set to auto review, Codex resolves an approval request itself — the `PermissionRequest` hook
//! fires, no modal is ever shown, and the tool proceeds. A raised hand there asks the user to
//! answer something they will never be shown, so the adapter reads the setting and says so in its
//! plan.
//!
//! Codex cannot pre-allocate a session id, and in the interactive TUI the thread is created lazily
//! on the first prompt submission — so a session the user opens without a task has no
//! `agent_session_id` until they type something. The id then arrives on the `SessionStart` hook
//! payload, and thread id and session id are the same value.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};
use crate::mcp;
use crate::protocol::Agent;

/// The events Octoboard's session states are derived from. `Interrupt` is included because it is
/// mutually exclusive with `Stop` and is the only signal of a cancelled turn any of the three
/// agents gives. `SubagentStart` / `SubagentStop` are deliberately not injected: they are noise
/// here, and their payloads' ids cannot be joined back to the parent turn.
const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Interrupt",
    "Stop",
];

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn binary(&self) -> &'static str {
        "codex"
    }

    fn preallocates_session_id(&self) -> bool {
        false
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> anyhow::Result<LaunchPlan> {
        let pinned = super::pinned_config_dir(spec, Agent::Codex)?;
        let mut args = Vec::new();

        // `resume` is a subcommand, so it comes before the overrides.
        if let Some(id) = spec.resume_agent_session_id {
            args.push("resume".to_string());
            args.push(id.to_string());
        }

        let canonical_cwd = std::fs::canonicalize(spec.cwd)
            .unwrap_or_else(|_| spec.cwd.to_path_buf())
            .to_string_lossy()
            .into_owned();
        args.push("-c".to_string());
        args.push(format!(
            "projects={{{}={{trust_level=\"trusted\"}}}}",
            toml_string(&canonical_cwd)
        ));

        let command = spec.hook_script.to_string_lossy().into_owned();
        for event in HOOK_EVENTS {
            args.push("-c".to_string());
            args.push(format!(
                "hooks.{event}=[{{hooks=[{{type=\"command\",command={},timeout={},async=true}}]}}]",
                toml_string(&command),
                HOOK_TIMEOUT_SECS
            ));
        }
        args.push("--dangerously-bypass-hook-trust".to_string());

        let (mcp_command, mcp_args) = super::mcp_server_command(spec);
        let key = mcp::SERVER_KEY;
        args.push("-c".to_string());
        args.push(format!(
            "mcp_servers.{key}.command={}",
            toml_string(&mcp_command)
        ));
        args.push("-c".to_string());
        args.push(format!(
            "mcp_servers.{key}.args=[{}]",
            mcp_args
                .iter()
                .map(|arg| toml_string(arg))
                .collect::<Vec<_>>()
                .join(",")
        ));
        // Octoboard's own tools are not a decision to put to the user: every one of them is
        // something the console session was told to do, and a modal for each would make
        // orchestration unusable.
        args.push("-c".to_string());
        args.push(format!(
            "mcp_servers.{key}.default_tools_approval_mode=\"auto\""
        ));

        args.push("-c".to_string());
        args.push(format!(
            "developer_instructions={}",
            toml_string(&mcp::role::role_description(spec.role, Agent::Codex))
        ));

        if spec.resume_agent_session_id.is_none() {
            if let Some(task) = spec.task {
                args.push(task.to_string());
            }
        }

        // Set only for a pinned directory; otherwise the shell snapshot's own value, if any, stands.
        let env = pinned
            .map(|dir| vec![("CODEX_HOME".to_string(), dir.to_string_lossy().into_owned())])
            .unwrap_or_default();

        Ok(LaunchPlan {
            args,
            env,
            resolves_approvals_itself: resolves_approvals_itself(spec, pinned),
            ..LaunchPlan::default()
        })
    }
}

/// Whether this user's Codex resolves approval requests without ever showing a dialog.
///
/// Read from the user's own `config.toml` — never written to it. The setting's shape is not pinned
/// down (it has been seen as a plain value and as a table), so the whole value is searched for the
/// auto-review marker rather than one key path being assumed. Unreadable or absent means "the user
/// will be asked", which is the safe way to be wrong: a hand raised needlessly is visible, while
/// one never raised leaves a session looking busy while it waits.
fn resolves_approvals_itself(spec: &LaunchSpec<'_>, pinned: Option<&Path>) -> bool {
    const AUTO_REVIEW: &str = "auto_review";
    let config = codex_home(spec, pinned).join("config.toml");
    let Ok(text) = std::fs::read_to_string(&config) else {
        return false;
    };
    // A `Table`, not a `Value`: in `toml` 1.x parsing into `Value` expects a bare value rather
    // than a whole document, and silently fails on the first key.
    let Ok(parsed) = text.parse::<toml::Table>() else {
        return false;
    };
    parsed
        .get("approvals_reviewer")
        .is_some_and(|value| mentions(value, AUTO_REVIEW))
}

/// Whether `wanted` appears as a string anywhere inside this value.
fn mentions(value: &toml::Value, wanted: &str) -> bool {
    match value {
        toml::Value::String(text) => text == wanted,
        toml::Value::Array(items) => items.iter().any(|item| mentions(item, wanted)),
        toml::Value::Table(table) => table.values().any(|item| mentions(item, wanted)),
        _ => false,
    }
}

/// The Codex home this launch will read: the session's pinned directory, else the user's own, read
/// from the launch environment rather than the daemon's — the daemon's own `CODEX_HOME` may belong
/// to an agent session it was started from.
fn codex_home(spec: &LaunchSpec<'_>, pinned: Option<&Path>) -> PathBuf {
    if let Some(dir) = pinned {
        return dir.to_path_buf();
    }
    if let Some(home) = spec
        .shell_env
        .get("CODEX_HOME")
        .filter(|home| !home.is_empty())
    {
        return PathBuf::from(home);
    }
    match spec.shell_env.get("HOME").filter(|home| !home.is_empty()) {
        Some(home) => PathBuf::from(home).join(".codex"),
        None => crate::paths::home_dir().join(".codex"),
    }
}

/// A TOML basic string. JSON string escaping is a subset of TOML's, so serde_json produces a valid
/// one and spares us a second escaping implementation.
fn toml_string(value: &str) -> String {
    json!(value).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;

    fn overrides(args: &[String]) -> Vec<String> {
        args.windows(2)
            .filter(|pair| pair[0] == "-c")
            .map(|pair| pair[1].clone())
            .collect()
    }

    #[test]
    fn passes_the_project_trust_table_whole_and_canonicalised() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        let canonical = std::fs::canonicalize(&fixture.cwd).expect("canonical path");
        let expected = format!(
            "projects={{{:?}={{trust_level=\"trusted\"}}}}",
            canonical.to_string_lossy()
        );
        // The whole table in one override: `-c` splits its key on `.`, so a dotted path with a
        // quoted segment is accepted and then silently ignored.
        assert!(
            overrides(&plan.args).contains(&expected),
            "expected {expected} among {:?}",
            overrides(&plan.args)
        );
    }

    #[test]
    fn injects_one_hook_override_per_event_and_gets_past_the_trust_gate() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        let overrides = overrides(&plan.args);
        for event in HOOK_EVENTS {
            assert_eq!(
                overrides
                    .iter()
                    .filter(|override_| override_.starts_with(&format!("hooks.{event}=")))
                    .count(),
                1,
                "exactly one override for {event}"
            );
        }
        for override_ in overrides.iter().filter(|o| o.starts_with("hooks.")) {
            assert!(override_.contains("async=true"));
            assert!(override_.contains(&format!("timeout={HOOK_TIMEOUT_SECS}")));
        }
        // Without clearing the hook trust gate an interactive session raises a blocking review
        // modal and a headless one hangs indefinitely.
        assert!(plan
            .args
            .iter()
            .any(|arg| arg == "--dangerously-bypass-hook-trust"));
    }

    #[test]
    fn a_resume_names_the_subcommand_first_and_still_re_injects() {
        let fixture = spec_fixture();
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some("thread-id");
        let plan = CodexAdapter.plan(&spec).expect("plan");
        assert_eq!(plan.args[0], "resume");
        assert_eq!(plan.args[1], "thread-id");
        // Every `-c` override is per-invocation and persisted nowhere, so a resume that omits them
        // loses the hooks and the MCP server.
        let overrides = overrides(&plan.args);
        assert!(overrides.iter().any(|o| o.starts_with("hooks.")));
        assert!(overrides
            .iter()
            .any(|o| o.starts_with("mcp_servers.octoboard.command=")));
        assert!(overrides
            .iter()
            .any(|o| o.starts_with("developer_instructions=")));
    }

    #[test]
    fn registers_the_mcp_server_as_a_command_with_its_tools_pre_approved() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        let overrides = overrides(&plan.args);
        assert!(overrides.contains(&format!(
            "mcp_servers.octoboard.command={:?}",
            fixture.self_exe
        )));
        let args = overrides
            .iter()
            .find(|o| o.starts_with("mcp_servers.octoboard.args="))
            .expect("the server's arguments");
        assert!(args.contains(&fixture.session_id));
        assert!(args.contains(&fixture.mcp_token));
        assert!(overrides
            .contains(&"mcp_servers.octoboard.default_tools_approval_mode=\"auto\"".to_string()));
    }

    /// `instructions` would replace the system prompt outright; `developer_instructions` adds a
    /// developer message and leaves the rest byte-identical.
    #[test]
    fn the_role_text_is_added_as_a_developer_message_not_a_replaced_system_prompt() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        let overrides = overrides(&plan.args);
        assert!(overrides.iter().any(|o| o.starts_with(&format!(
            "developer_instructions={}",
            toml_string(&mcp::role::role_description(fixture.role, Agent::Codex))
        ))));
        assert!(!overrides.iter().any(|o| o.starts_with("instructions=")));
    }

    /// A hand raised for an approval the user is never shown can never be cleared by them, so the
    /// setting has to be read before the promise is made.
    #[test]
    fn auto_review_in_the_users_own_config_is_noticed() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("codex-home");
        std::fs::create_dir_all(&home).expect("codex home");
        fixture.shell_env.insert(
            "CODEX_HOME".to_string(),
            home.to_string_lossy().into_owned(),
        );

        // No configuration at all: the user gets asked, which is the safe way to be wrong.
        assert!(
            !CodexAdapter
                .plan(&fixture.spec())
                .expect("plan")
                .resolves_approvals_itself
        );

        std::fs::write(
            home.join("config.toml"),
            "approvals_reviewer = \"auto_review\"\n",
        )
        .expect("config");
        assert!(
            CodexAdapter
                .plan(&fixture.spec())
                .expect("plan")
                .resolves_approvals_itself
        );

        // The setting has been seen as a table as well as a plain value, so neither shape may be
        // assumed.
        std::fs::write(
            home.join("config.toml"),
            "[approvals_reviewer]\nmode = \"auto_review\"\n",
        )
        .expect("config");
        assert!(
            CodexAdapter
                .plan(&fixture.spec())
                .expect("plan")
                .resolves_approvals_itself
        );

        std::fs::write(home.join("config.toml"), "approvals_reviewer = \"ask\"\n").expect("config");
        assert!(
            !CodexAdapter
                .plan(&fixture.spec())
                .expect("plan")
                .resolves_approvals_itself
        );
    }

    /// The approval setting is read from the Codex home in effect, which is the pinned directory
    /// when there is one — not the shell's or the default one.
    #[test]
    fn the_approval_setting_is_read_from_the_pinned_codex_home() {
        let mut fixture = spec_fixture();
        let shell_home = fixture.scratch.join("shell-codex-home");
        let pinned = fixture.scratch.join("pinned-codex-home");
        for dir in [&shell_home, &pinned] {
            std::fs::create_dir_all(dir).expect("codex home");
        }
        fixture.shell_env.insert(
            "CODEX_HOME".to_string(),
            shell_home.to_string_lossy().into_owned(),
        );
        std::fs::write(
            shell_home.join("config.toml"),
            "approvals_reviewer = \"auto_review\"\n",
        )
        .expect("config");
        let resolves = |fixture: &crate::adapter::tests::SpecFixture| {
            CodexAdapter
                .plan(&fixture.spec())
                .expect("plan")
                .resolves_approvals_itself
        };
        assert!(resolves(&fixture));

        // The pinned directory replaces the shell's: its own (empty) configuration decides.
        fixture.config_dir = Some(pinned.clone());
        assert!(!resolves(&fixture));
        std::fs::write(
            pinned.join("config.toml"),
            "approvals_reviewer = \"auto_review\"\n",
        )
        .expect("config");
        assert!(resolves(&fixture));
    }

    #[test]
    fn a_pinned_codex_home_is_exported_and_an_unpinned_launch_adds_nothing() {
        let mut fixture = spec_fixture();
        assert!(CodexAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .env
            .is_empty());

        let dir = fixture.scratch.join("codex-alt");
        std::fs::create_dir_all(&dir).expect("config dir");
        fixture.config_dir = Some(dir.clone());
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            plan.env,
            vec![("CODEX_HOME".to_string(), dir.to_string_lossy().into_owned())]
        );
    }

    #[test]
    fn a_pinned_codex_home_that_has_gone_refuses_the_launch() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        let err = CodexAdapter.plan(&fixture.spec()).err().expect("refused");
        let message = err.to_string();
        assert!(message.contains("Codex"), "{message}");
        assert!(message.contains("missing"), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }

    #[test]
    fn never_pre_allocates_a_session_id() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        assert!(!CodexAdapter.preallocates_session_id());
        assert!(!plan.args.iter().any(|arg| arg.contains("session-id")));
    }
}
