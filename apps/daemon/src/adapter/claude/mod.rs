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
//! read out of Claude Code's global config file instead of matched in terminal text. Read, never
//! written: Octoboard does not edit that file. The trust screen itself, which a fresh directory
//! raises, is recognised from terminal text and answered with keystrokes once the user has agreed
//! to that — see `crate::trust`.
//!
//! An approval mode chosen for the account is passed as `--permission-mode` on every launch and
//! resume, and none is passed for an account that follows its own configuration. Approve everything
//! also adds `skipDangerousModePermissionPrompt` to the `--settings` JSON: Octoboard asks the user
//! for that consent in Settings, and the flag-borne key keeps Claude Code's own blocking warning
//! from appearing without writing anything into the account's configuration files.
//!
//! A console can pin its sessions to another Claude Code configuration directory
//! (`CLAUDE_CONFIG_DIR`). That directory holds the conversation transcripts as well as the global
//! config file, so everything here that reads or relaunches follows it: the variable is set for the
//! launch, and the trust state is read from the file inside it.
//!
//! Claude Code's transcript records no context window; only the input of its status line carries
//! one. So the `--settings` JSON also sets `statusLine`, to a command that hands the window to the
//! daemon and then runs the user's own status line, resolved afresh on every run, with the same
//! input, its output and exit status passed on unchanged (`crate::status_line_mode`). A
//! `statusLine` from the flags outranks the user's and the project's, so carrying theirs out is
//! this command's job, and their layout fields (`padding`, `refreshInterval`,
//! `hideVimModeIndicator`) are carried into the injected one when well-typed. What cannot be carried over is the status line's absence: with any status
//! line configured Claude Code drops its "? for shortcuts" footer hint, and in fullscreen keeps a
//! blank row for the line (read out of Claude Code 2.1.306), so a user with none of their own sees
//! those two changes. That is accepted as the price of the window. A managed `statusLine` outranks
//! the flags, and then this one never runs and the session reports no window.

pub mod settings;

use std::path::Path;

use serde_json::json;

use super::{
    hook_command, hook_url, shell_quote, AgentAdapter, AgentVersion, LaunchPlan, LaunchSpec,
    NamedTitle, HOOK_TIMEOUT_SECS,
};
use crate::mcp;
use crate::protocol::{Agent, ClaudeApprovalMode};
use settings::{
    http_hooks_allowed, settings_files, untrusted_workspace_notice, user_config_dir,
    user_status_line,
};

/// The events that carry the session states Octoboard shows. `StopFailure` is registered next to
/// `Stop` because the two are mutually exclusive — a turn ending in an API error fires only
/// `StopFailure`, and a daemon keying "finished" on `Stop` alone leaves that session looking busy
/// forever. `PostToolBatch` is here because a tool call rejected by Claude Code's own
/// pre-execution guard fires only that one, skipping `PreToolUse` and both post events.
/// `SubagentStart` and `SubagentStop` keep the background tasks current while the session is idle
/// and a subagent works (see `crate::hooks::background_tasks_change`).
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
    "SubagentStart",
    "SubagentStop",
];

/// The first Claude Code that takes a `"type": "http"` hook handler, which posts the event's JSON
/// to a URL from inside the agent's own process. Older ones only run commands.
const HTTP_HOOKS_SINCE: AgentVersion = (2, 1, 63);

/// The registered events Claude Code skips an HTTP handler for (it runs only commands on
/// `SessionStart` and `Setup`; measured on 2.1.307), so they keep the command handler whatever the
/// rest use. `SessionStart` is the only hook a launch or resume with no prompt sends, and the
/// daemon holds every write into the session until a first hook arrives (see `crate::trust`), so
/// without it such a session would hold its messages until the user typed a prompt.
const COMMAND_ONLY_EVENTS: &[&str] = &["SessionStart"];

/// The status line is injected only for a Claude Code known to be at least this version, well
/// inside the releases that run a `statusLine` command; one whose input carries no context window
/// still passes the user's own status line through and reports nothing.
const STATUS_LINE_SINCE: AgentVersion = (2, 0, 0);

pub struct ClaudeAdapter;

/// Reads a version that is exactly `major.minor.patch` and nothing else.
fn parse_version(text: &str) -> Option<AgentVersion> {
    let mut parts = text.split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

impl ClaudeAdapter {
    /// The `--settings` JSON. A Claude Code that takes HTTP hooks posts each event to the
    /// daemon itself, so a hook costs no process at all, except for [`COMMAND_ONLY_EVENTS`];
    /// with an older or unknown version, or when a settings file's `allowedHttpHookUrls` leaves
    /// the daemon's URL out (Claude Code then skips every HTTP hook silently), each event runs
    /// a command instead. `async` keeps a slow command off the turn's critical path (a 30 s
    /// hook took a turn from 6.8 s to 36.7 s without it) and `timeout` bounds it regardless; an
    /// HTTP hook has no `async`, and is kept off the critical path by the route answering
    /// before the event is applied (see `server::hook_callback`). Claude Code runs a command
    /// through a shell, so the daemon is exec'd by the command itself rather than through the
    /// per-session hook script, which would be one more process to start per event.
    ///
    /// A daemon that cannot be reached, or does not answer within the timeout, shows a
    /// non-blocking error notice in Claude Code for an HTTP hook, where the command form swallows
    /// it. That is accepted: the daemon owns the session's terminal, so one that is gone has taken
    /// the session with it.
    fn settings_json(spec: &LaunchSpec<'_>, pinned: Option<&Path>) -> String {
        let url = hook_url(spec.session_id, spec.daemon_port);
        let http = spec.agent_version.is_some_and(|v| v >= HTTP_HOOKS_SINCE)
            && http_hooks_allowed(&url, &settings_files(spec, pinned));
        let command = json!({
            "type": "command",
            "command": hook_command(spec.self_exe, spec.session_id, spec.daemon_port),
            "async": true,
            "timeout": HOOK_TIMEOUT_SECS,
        });
        let http_handler = json!({
            "type": "http",
            "url": url,
            "timeout": HOOK_TIMEOUT_SECS,
        });
        let mut hooks = serde_json::Map::new();
        for event in HOOK_EVENTS {
            let handler = if http && !COMMAND_ONLY_EVENTS.contains(event) {
                &http_handler
            } else {
                &command
            };
            let mut group = json!({ "hooks": [handler] });
            // Only `idle_prompt` is read (see `crate::hooks`); the matcher keeps the other
            // notifications from being delivered for nothing.
            if *event == "Notification" {
                group["matcher"] = json!("idle_prompt");
            }
            hooks.insert((*event).to_string(), json!([group]));
        }
        let mut settings = json!({ "hooks": hooks });
        if spec.approval_mode == Some(ClaudeApprovalMode::Bypass) {
            settings["skipDangerousModePermissionPrompt"] = json!(true);
        }
        if spec.agent_version.is_some_and(|v| v >= STATUS_LINE_SINCE) {
            settings["statusLine"] = Self::status_line(spec, pinned);
        }
        settings.to_string()
    }

    /// The injected `statusLine`: the command that runs `octoboardd status-line`, with the layout
    /// fields of the user's own status line as they stand at launch. Only the fields Claude Code
    /// 2.1.306 knows are carried, each only with a value of the type its schema takes, so nothing
    /// in the user's files can make the flag's settings, hooks and all, fail to validate. The
    /// command is given the directories the user's settings files are in, worked out here as for
    /// the rest of this launch, and reads the files themselves again on every run.
    fn status_line(spec: &LaunchSpec<'_>, pinned: Option<&Path>) -> serde_json::Value {
        let user_dir = user_config_dir(spec, pinned);
        let own = user_status_line(&user_dir, spec.cwd).unwrap_or_default();
        let mut line = serde_json::Map::new();
        line.insert("type".to_owned(), json!("command"));
        line.insert(
            "command".to_owned(),
            json!(format!(
                "exec {} status-line --session {} --port {} --cwd {} --config-dir {}",
                shell_quote(spec.self_exe),
                shell_quote(spec.session_id),
                spec.daemon_port,
                shell_quote(&spec.cwd.to_string_lossy()),
                shell_quote(&user_dir.to_string_lossy()),
            )),
        );
        for key in ["padding", "refreshInterval", "hideVimModeIndicator"] {
            let valid = |value: &serde_json::Value| match key {
                "padding" => value.is_number(),
                "refreshInterval" => value.as_f64().is_some_and(|seconds| seconds >= 1.0),
                _ => value.is_boolean(),
            };
            if let Some(value) = own.get(key).filter(|value| valid(value)) {
                line.insert(key.to_owned(), value.clone());
            }
        }
        serde_json::Value::Object(line)
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

    /// Two layouts, each matched exactly, since a guess that is wrong switches hook mechanisms on
    /// a Claude Code that cannot take them. A native install keeps each release as a file named
    /// for its version in a directory called `versions` (`~/.local/share/claude/versions/2.1.301`,
    /// which `~/.local/bin/claude` links to). A package install keeps the version in the
    /// `package.json` of the nearest `claude-code` directory the binary resolves into, looked for
    /// no further out than the enclosing `node_modules`. Any other layout, or a path that cannot be
    /// followed, is `None`.
    fn installed_version(&self, binary: &str) -> Option<AgentVersion> {
        let resolved = std::fs::canonicalize(binary).ok()?;
        let parent = resolved.parent()?;
        if parent.file_name().is_some_and(|name| name == "versions") {
            return parse_version(resolved.file_name()?.to_str()?);
        }
        for dir in parent.ancestors() {
            let name = dir.file_name()?;
            if name == "node_modules" {
                return None;
            }
            if name == "claude-code" {
                let manifest = std::fs::read_to_string(dir.join("package.json")).ok()?;
                let manifest: serde_json::Value = serde_json::from_str(&manifest).ok()?;
                return parse_version(manifest.get("version")?.as_str()?);
            }
        }
        None
    }

    /// Claude Code leads its title with a status glyph that changes as it works, and shows its own
    /// name until the conversation has a title.
    fn task_title(&self, raw: &str, _project_names: &[String]) -> Option<String> {
        let raw = raw.trim();
        let mut chars = raw.chars();
        let title = match chars.next() {
            Some(glyph) if !glyph.is_ascii() && !glyph.is_alphanumeric() => chars.as_str().trim(),
            _ => raw,
        };
        (!title.is_empty() && title != "Claude Code").then(|| title.to_owned())
    }

    /// The transcript's `custom-title` line is the user's own rename and outranks the `ai-title`
    /// line Claude Code writes itself.
    fn named_title(&self, line: &str) -> Option<NamedTitle> {
        let (kind, key, precedence) = if line.contains("\"custom-title\"") {
            ("custom-title", "customTitle", 2)
        } else if line.contains("\"ai-title\"") {
            ("ai-title", "aiTitle", 1)
        } else {
            return None;
        };
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        (value.get("type")?.as_str()? == kind).then_some(())?;
        Some(NamedTitle {
            text: value.get(key)?.as_str()?.to_owned(),
            precedence,
        })
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> anyhow::Result<LaunchPlan> {
        let pinned = super::pinned_config_dir(spec, Agent::Claude)?;
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
        args.push(Self::settings_json(spec, pinned));
        args.push("--mcp-config".to_string());
        args.push(Self::mcp_config_json(spec));
        args.push("--append-system-prompt".to_string());
        args.push(mcp::role::role_description(
            spec.role,
            spec.owner.as_ref(),
            Agent::Claude,
        ));
        // Last, after every list-valued flag has closed. All are global flags, so they sit
        // alongside `--resume` too.
        if let Some(mode) = spec.approval_mode {
            args.push("--permission-mode".to_string());
            // `default` rather than `manual`: every Claude Code version accepts it, and it is what
            // the hooks and the transcript report; `manual` exists only from 2.1.200.
            args.push(
                match mode {
                    ClaudeApprovalMode::Auto => "auto",
                    ClaudeApprovalMode::Manual => "default",
                    ClaudeApprovalMode::Bypass => "bypassPermissions",
                }
                .to_string(),
            );
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        // Claude Code quietly lowers `max` and `xhigh` to `high` on a model that does not support
        // them.
        if let Some(effort) = spec.effort {
            args.push("--effort".to_string());
            args.push(effort.to_string());
        }

        let mut env = Vec::new();
        if let Some(dir) = pinned {
            env.push((
                "CLAUDE_CONFIG_DIR".to_string(),
                dir.to_string_lossy().into_owned(),
            ));
        }
        // Claude Code writes auto memory into `projects/<key>/memory/` without asking only while
        // the path is spelled inside the account's own directory, so through a linked
        // `projects/` it asks, or refuses outright in a headless run. Moving the memory root to
        // the resolved shared directory restores that, and keeps memory where the default
        // account keeps it. An undocumented internal variable, set for this launch only and
        // never in the user's settings.
        if let Some(dir) = spec.shared_default_dir {
            env.push((
                "CLAUDE_CODE_REMOTE_MEMORY_DIR".to_string(),
                dir.to_string_lossy().into_owned(),
            ));
        }

        Ok(LaunchPlan {
            args,
            env,
            notice: untrusted_workspace_notice(spec, pinned),
            ..LaunchPlan::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;

    #[test]
    fn terminal_titles_lose_the_status_glyph_and_the_placeholder() {
        let task = |raw| ClaudeAdapter.task_title(raw, &[]);
        assert_eq!(task("✳ Claude Code"), None);
        assert_eq!(task("◐ Claude Code"), None);
        assert_eq!(
            task("◐ Pseudo terminal explanation").as_deref(),
            Some("Pseudo terminal explanation")
        );
        assert_eq!(
            task("✳ Pseudo terminal explanation").as_deref(),
            Some("Pseudo terminal explanation")
        );
        assert_eq!(task("Fix the build").as_deref(), Some("Fix the build"));
        assert_eq!(task(""), None);
    }

    #[test]
    fn the_transcripts_custom_title_outranks_its_ai_title() {
        let title = |line| {
            ClaudeAdapter
                .named_title(line)
                .map(|t| (t.precedence, t.text))
        };
        assert_eq!(
            title(r#"{"type":"ai-title","aiTitle":"Generated","sessionId":"x"}"#),
            Some((1, "Generated".to_owned()))
        );
        assert_eq!(
            title(r#"{"type":"custom-title","customTitle":"Mine","sessionId":"x"}"#),
            Some((2, "Mine".to_owned()))
        );
        assert_eq!(
            title(r#"{"type":"user","message":"an ai-title mention"}"#),
            None
        );
    }

    /// With no model or effort set, neither flag is passed, so Claude Code keeps its own choice.
    #[test]
    fn no_model_or_effort_adds_no_flags() {
        let fixture = spec_fixture();
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        for flag in ["--model", "--effort"] {
            assert!(!plan.args.iter().any(|arg| arg == flag), "{flag}");
        }
    }

    #[test]
    fn a_model_and_effort_are_appended_after_every_other_flag() {
        let mut fixture = spec_fixture();
        fixture.model = Some("opus".to_string());
        fixture.effort = Some("high".to_string());
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            plan.args[plan.args.len() - 4..],
            ["--model", "opus", "--effort", "high"]
        );
    }

    /// The flag follows the choice on a fresh launch and a resume alike; following the account
    /// configuration passes none, and only Approve everything carries the consent key in
    /// `--settings`.
    #[test]
    fn the_approval_mode_is_passed_on_fresh_launches_and_resumes() {
        let mut fixture = spec_fixture();
        fixture.model = Some("opus".to_string());
        let cases = [
            (None, None, false),
            (Some(ClaudeApprovalMode::Auto), Some("auto"), false),
            (Some(ClaudeApprovalMode::Manual), Some("default"), false),
            (
                Some(ClaudeApprovalMode::Bypass),
                Some("bypassPermissions"),
                true,
            ),
        ];
        for (mode, flag, bypass) in cases {
            for resume in [None, Some("conversation-1")] {
                let mut spec = fixture.spec();
                spec.approval_mode = mode;
                spec.resume_agent_session_id = resume;
                let plan = ClaudeAdapter.plan(&spec).expect("plan");
                let position = plan.args.iter().position(|arg| arg == "--permission-mode");
                assert_eq!(
                    position.map(|at| plan.args[at + 1].as_str()),
                    flag,
                    "{mode:?} {resume:?}"
                );
                if let Some(at) = position {
                    assert_eq!(
                        plan.args
                            .iter()
                            .filter(|arg| *arg == "--permission-mode")
                            .count(),
                        1
                    );
                    assert!(
                        at > plan
                            .args
                            .iter()
                            .position(|arg| arg == "--mcp-config")
                            .unwrap()
                    );
                }
                let at = plan
                    .args
                    .iter()
                    .position(|arg| arg == "--settings")
                    .unwrap();
                let settings: serde_json::Value = serde_json::from_str(&plan.args[at + 1]).unwrap();
                assert_eq!(
                    settings.get("skipDangerousModePermissionPrompt"),
                    bypass.then_some(&serde_json::Value::Bool(true)),
                    "{mode:?} {resume:?}"
                );
                assert!(settings["hooks"].is_object());
            }
        }
    }

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

    /// A console's setting reaches Claude Code only as an environment entry in the plan; with none
    /// set the plan adds nothing, so the shell snapshot's own value stays in force. Layering the
    /// entry over the snapshot is `term::launch`'s job, not the adapter's.
    #[test]
    fn a_pinned_config_dir_is_exported_and_an_unpinned_launch_adds_nothing() {
        let mut fixture = spec_fixture();
        assert!(ClaudeAdapter
            .plan(&fixture.spec())
            .expect("plan")
            .env
            .is_empty());

        let dir = fixture.scratch.join("claude-alt");
        std::fs::create_dir_all(&dir).expect("config dir");
        fixture.config_dir = Some(dir.clone());
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            plan.env,
            vec![(
                "CLAUDE_CONFIG_DIR".to_string(),
                dir.to_string_lossy().into_owned()
            )]
        );
    }

    /// Auto memory is pointed at the shared directory only for a launch of an account that
    /// shares it, and only through the launch's environment.
    #[test]
    fn a_sharing_accounts_launch_points_auto_memory_at_the_shared_directory() {
        let mut fixture = spec_fixture();
        let dir = fixture.scratch.join("claude-alt");
        std::fs::create_dir_all(&dir).expect("config dir");
        fixture.config_dir = Some(dir);
        let memory = |plan: LaunchPlan| {
            plan.env
                .into_iter()
                .find(|(key, _)| key == "CLAUDE_CODE_REMOTE_MEMORY_DIR")
                .map(|(_, value)| value)
        };
        assert_eq!(
            memory(ClaudeAdapter.plan(&fixture.spec()).expect("plan")),
            None
        );

        let shared = fixture.scratch.join("default");
        fixture.shared_default_dir = Some(shared.clone());
        assert_eq!(
            memory(ClaudeAdapter.plan(&fixture.spec()).expect("plan")),
            Some(shared.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn a_pinned_config_dir_that_has_gone_refuses_resuming_a_conversation() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some("agent-side-id");
        let err = ClaudeAdapter.plan(&spec).err().expect("refused");
        let message = err.to_string();
        assert!(message.contains("Claude"), "{message}");
        assert!(message.contains("missing"), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }

    /// A fresh session, and one that never had a turn (no `resume_agent_session_id`), launch into a
    /// vanished pinned directory rather than being refused — that is what lets the agent create it.
    #[test]
    fn a_pinned_config_dir_that_has_gone_is_fine_with_no_conversation_to_resume() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        ClaudeAdapter.plan(&fixture.spec()).expect("not refused");
    }

    /// The `hooks` table of the `--settings` this launch passes.
    fn injected_hooks(fixture: &crate::adapter::tests::SpecFixture) -> serde_json::Value {
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        let settings_index = plan
            .args
            .iter()
            .position(|arg| arg == "--settings")
            .expect("--settings is passed");
        let settings: serde_json::Value =
            serde_json::from_str(&plan.args[settings_index + 1]).expect("the settings are JSON");
        settings["hooks"].clone()
    }

    #[test]
    fn injects_every_hook_event_as_a_fast_silent_command() {
        let fixture = spec_fixture();
        let hooks = injected_hooks(&fixture);
        let hooks = hooks.as_object().expect("a hooks table");
        assert_eq!(hooks.len(), HOOK_EVENTS.len());
        for event in HOOK_EVENTS {
            let handler = &hooks[*event][0]["hooks"][0];
            assert_eq!(handler["type"], "command");
            assert_eq!(handler["async"], true);
            assert_eq!(handler["timeout"], HOOK_TIMEOUT_SECS);
            assert_eq!(
                handler["command"].as_str().unwrap(),
                "exec '/opt/octoboard/octoboardd' hook --session 'session-1' --port 4321 2>/dev/null"
            );
            // Only `idle_prompt` of the notifications is read.
            assert_eq!(
                hooks[*event][0].get("matcher").and_then(|m| m.as_str()),
                (*event == "Notification").then_some("idle_prompt")
            );
        }
    }

    /// Both layouts match exactly, and a binary whose install tells nothing has no version.
    #[test]
    fn a_binary_version_comes_from_its_install_location() {
        let root = crate::test_support::ScratchDir::new("binary-version");
        let version_of =
            |path: &std::path::Path| ClaudeAdapter.installed_version(&path.to_string_lossy());
        std::fs::create_dir_all(root.join("bin")).expect("bin");

        let versions = root.join("share/versions");
        std::fs::create_dir_all(&versions).expect("versions");
        std::fs::write(versions.join("2.1.301"), "").expect("binary");
        let native = root.join("bin/claude");
        std::os::unix::fs::symlink(versions.join("2.1.301"), &native).expect("link");
        assert_eq!(version_of(&native), Some((2, 1, 301)));

        // A file in a directory of another name, or one that is not named exactly for a version,
        // is not read as one.
        let elsewhere = root.join("share/builds");
        std::fs::create_dir_all(&elsewhere).expect("builds");
        std::fs::write(elsewhere.join("2.1.301"), "").expect("binary");
        assert_eq!(version_of(&elsewhere.join("2.1.301")), None);
        for name in ["v2.1.301", "2.1.301-beta", "2.1", "2.1.3.1"] {
            std::fs::write(versions.join(name), "").expect("binary");
            assert_eq!(version_of(&versions.join(name)), None, "{name}");
        }

        let package = root.join("lib/node_modules/@anthropic-ai/claude-code");
        std::fs::create_dir_all(package.join("bin")).expect("package");
        std::fs::write(package.join("package.json"), r#"{"version":"2.1.100"}"#).expect("manifest");
        std::fs::write(package.join("bin/claude.js"), "").expect("entry");
        let npm = root.join("bin/claude-npm");
        std::os::unix::fs::symlink(package.join("bin/claude.js"), &npm).expect("link");
        assert_eq!(version_of(&npm), Some((2, 1, 100)));

        // The walk stops at `node_modules`, so another package's manifest further out is not read.
        let other = root.join("lib/node_modules/other");
        std::fs::create_dir_all(&other).expect("other");
        std::fs::write(root.join("lib/package.json"), r#"{"version":"9.9.9"}"#).expect("manifest");
        std::fs::write(other.join("cli.js"), "").expect("entry");
        assert_eq!(version_of(&other.join("cli.js")), None);

        let plain = root.join("bin/plain");
        std::fs::write(&plain, "").expect("binary");
        assert_eq!(version_of(&plain), None);
    }

    /// A Claude Code that takes HTTP hooks gets the daemon's hook route, with no process behind any
    /// event but the ones it runs only commands for; one older than that, or of a version nobody
    /// could tell, keeps the command for every event.
    #[test]
    fn http_hooks_are_injected_from_the_version_that_takes_them() {
        let mut fixture = spec_fixture();
        for (version, http) in [
            (Some((2, 1, 63)), true),
            (Some((2, 1, 301)), true),
            (Some((3, 0, 0)), true),
            (Some((2, 1, 62)), false),
            (Some((1, 9, 99)), false),
            (None, false),
        ] {
            fixture.agent_version = version;
            let hooks = injected_hooks(&fixture);
            for event in HOOK_EVENTS {
                let handler = &hooks[*event][0]["hooks"][0];
                assert_eq!(handler["timeout"], HOOK_TIMEOUT_SECS);
                if http && *event != "SessionStart" {
                    assert_eq!(handler["type"], "http", "{version:?}");
                    assert_eq!(
                        handler["url"], "http://127.0.0.1:4321/hook/session-1",
                        "{version:?}"
                    );
                    assert!(handler.get("async").is_none(), "{version:?}");
                    assert!(handler.get("command").is_none(), "{version:?}");
                } else {
                    // `SessionStart` among them, which Claude Code runs no HTTP handler for: it is
                    // the one hook a launch or resume with no prompt sends.
                    assert_eq!(handler["type"], "command", "{event} {version:?}");
                    assert_eq!(handler["async"], true, "{event} {version:?}");
                    assert_eq!(
                        handler["command"],
                        "exec '/opt/octoboard/octoboardd' hook --session 'session-1' --port 4321 \
                         2>/dev/null",
                        "{event} {version:?}"
                    );
                }
                assert_eq!(
                    hooks[*event][0].get("matcher").and_then(|m| m.as_str()),
                    (*event == "Notification").then_some("idle_prompt")
                );
            }
        }
    }

    /// The `--settings` this launch passes.
    fn injected_settings(fixture: &crate::adapter::tests::SpecFixture) -> serde_json::Value {
        let plan = ClaudeAdapter.plan(&fixture.spec()).expect("plan");
        let at = plan
            .args
            .iter()
            .position(|arg| arg == "--settings")
            .expect("--settings is passed");
        serde_json::from_str(&plan.args[at + 1]).expect("the settings are JSON")
    }

    /// The status line is injected only for a Claude Code known to run one, and names the
    /// directories its settings are read from: the pinned one when there is one.
    #[test]
    fn the_status_line_is_injected_from_the_version_that_runs_one() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("home");
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        for (version, injected) in [
            (Some((2, 0, 0)), true),
            (Some((2, 1, 306)), true),
            (Some((1, 0, 99)), false),
            (None, false),
        ] {
            fixture.agent_version = version;
            assert_eq!(
                injected_settings(&fixture).get("statusLine").is_some(),
                injected,
                "{version:?}"
            );
        }
        fixture.agent_version = Some((2, 1, 306));
        let command = |fixture: &crate::adapter::tests::SpecFixture| {
            injected_settings(fixture)["statusLine"]["command"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let cwd = fixture.cwd.to_string_lossy().into_owned();
        assert_eq!(
            command(&fixture),
            format!(
                "exec '/opt/octoboard/octoboardd' status-line --session 'session-1' --port 4321 \
                 --cwd '{cwd}' --config-dir '{}'",
                home.join(".claude").display()
            )
        );
        let pinned = fixture.scratch.join("pinned");
        std::fs::create_dir_all(&pinned).expect("pinned");
        fixture.config_dir = Some(pinned.clone());
        assert!(command(&fixture).ends_with(&format!("--config-dir '{}'", pinned.display())));
    }

    /// The layout fields of the user's own status line are carried into the injected one, so its
    /// layout is kept; with none of their own, only the type and command are set.
    #[test]
    fn the_users_status_line_fields_are_carried_into_the_injected_one() {
        let mut fixture = spec_fixture();
        fixture.agent_version = Some((2, 1, 306));
        let user = fixture.scratch.join("user");
        std::fs::create_dir_all(&user).expect("user directory");
        fixture.config_dir = Some(user.clone());
        let line = |fixture: &crate::adapter::tests::SpecFixture| {
            let mut line = injected_settings(fixture)["statusLine"].clone();
            assert_eq!(line["type"], "command");
            assert!(line["command"].as_str().unwrap().contains(" status-line "));
            let line = line.as_object_mut().unwrap();
            line.remove("type");
            line.remove("command");
            serde_json::Value::Object(line.clone())
        };
        assert_eq!(line(&fixture), json!({}));

        std::fs::write(
            user.join("settings.json"),
            json!({"statusLine":{"type":"command","command":"mine","padding":1,
                "hideVimModeIndicator":true}})
            .to_string(),
        )
        .expect("user settings");
        std::fs::create_dir_all(fixture.cwd.join(".claude")).expect("project settings");
        std::fs::write(
            fixture.cwd.join(".claude/settings.local.json"),
            json!({"statusLine":{"padding":0,"refreshInterval":10}}).to_string(),
        )
        .expect("local settings");
        assert_eq!(
            line(&fixture),
            json!({"padding":0,"hideVimModeIndicator":true,"refreshInterval":10})
        );
    }

    /// A field Claude Code does not know, or a known one with a value its schema refuses, is left
    /// out, so the user's files can never make the flag's settings invalid.
    #[test]
    fn only_known_status_line_fields_of_the_right_type_are_carried() {
        let mut fixture = spec_fixture();
        fixture.agent_version = Some((2, 1, 306));
        let user = fixture.scratch.join("user");
        std::fs::create_dir_all(&user).expect("user directory");
        fixture.config_dir = Some(user.clone());
        let carried = |own: serde_json::Value| {
            std::fs::write(
                user.join("settings.json"),
                json!({ "statusLine": own }).to_string(),
            )
            .expect("user settings");
            let line = injected_settings(&fixture)["statusLine"].clone();
            let mut keys: Vec<_> = line.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            (keys, line)
        };
        let (keys, line) = carried(json!({"type":"command","command":"mine","padding":"2",
            "refreshInterval":0.5,"hideVimModeIndicator":"yes","shell":"bash","timeout":5}));
        assert_eq!(keys, ["command", "type"]);
        assert_eq!(line["type"], "command");
        let (keys, line) = carried(json!({"type":"other","padding":2.5,"refreshInterval":1,
            "hideVimModeIndicator":false}));
        assert_eq!(
            keys,
            [
                "command",
                "hideVimModeIndicator",
                "padding",
                "refreshInterval",
                "type"
            ]
        );
        assert_eq!(
            (&line["type"], &line["padding"], &line["refreshInterval"]),
            (&json!("command"), &json!(2.5), &json!(1))
        );
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
            mcp::role::role_description(fixture.role, fixture.owner.as_ref(), Agent::Claude)
        );
    }
}
