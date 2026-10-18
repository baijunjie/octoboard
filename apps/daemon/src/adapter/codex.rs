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
//! Codex's update check is turned off for each launch (`check_for_update_on_startup=false`): its
//! "Update now / Skip" prompt would otherwise sit in front of the trust screen Octoboard watches
//! for. Only the launch is affected; the user's own setting is left as it is.
//!
//! Two kinds of trust are involved, and they are handled differently:
//!
//! - **Project trust** is not passed. In a git repository Codex has not been told to trust, it
//!   shows its own "Trust this folder?" confirmation, which Octoboard presses under the user's
//!   permission (`crate::trust`); Codex then records `trust_level = "trusted"` for the repository
//!   root in its own `config.toml`, so a later launch is trusted from the start. A per-launch
//!   `projects` override would be saved nowhere, and would mark the folder trusted on the user's
//!   behalf.
//! - **Hook trust** is a launch prerequisite. Codex gates any new or changed hook behind a
//!   persisted trust hash. Without it an interactive session raises a blocking review modal and no
//!   hook runs, and a headless one hangs indefinitely with no output at all.
//!   `--dangerously-bypass-hook-trust` is the verified way through; despite the name it weakens
//!   neither the sandbox nor the approval policy, it only skips the review of hooks Octoboard
//!   generated itself. The cost — two warning lines on every launch — is permanent short of
//!   redesigning the injection so the hook command is session-independent: the hash is taken over
//!   the handler definition, which includes the hook command, and that command is the session's
//!   own hook script path, so seeding `hooks.state` with hashes shipped with the adapter cannot
//!   work — no hash captured once could ever match a different session's.
//!
//! The effective `approvals_reviewer` changes what Octoboard may promise: with it set to auto review,
//! the `PermissionRequest` hook fires for requests resolved without showing a modal. The adapter
//! resolves the account override or reads the user's setting so those events do not raise a hand.
//! Actual action prompts are observed separately from the terminal activity title, since automatic
//! review does not rule out manual approvals.
//!
//! Codex cannot pre-allocate a session id, and in the interactive TUI the thread is created lazily
//! on the first prompt submission — so a session the user opens without a task has no
//! `agent_session_id` until they type something. The id then arrives on the `SessionStart` hook
//! payload, and thread id and session id are the same value.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};
use crate::mcp;
use crate::protocol::{error_code, Agent, CodedError};

/// The events Octoboard's session states are derived from. `Interrupt` is included because it is
/// mutually exclusive with `Stop` and is the only signal of a cancelled turn any of the three
/// agents gives. `SubagentStart` / `SubagentStop` are deliberately not injected: they are noise
/// here, and their payloads' ids cannot be joined back to the parent turn. `PreToolUse` is kept
/// although Codex's mapping reads no status from it, so that the deliberate `None` arm in
/// `crate::hooks` has an event to be about: drop the event and the arm goes with it, and with it
/// the only written record that the event races the same call's `PermissionRequest`. The other
/// adapters map it, so injecting it for Codex is the natural thing to do next, and the race would
/// come back silently. The cost is one hook process per tool call, and the event still reaches
/// `trust::on_hook` and `set_agent_session_id` as every event does.
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

/// What the user typed, from one parsed line of a rollout: the `user_message` event of older Codex,
/// or the `item_completed` event of a `UserMessage` item of newer. The user-role response items
/// beside them include the environment context and `AGENTS.md` instructions Codex adds itself.
pub(crate) fn typed_text(value: &serde_json::Value) -> Option<String> {
    if value.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = value.get("payload")?;
    match payload.get("type")?.as_str()? {
        "user_message" => Some(payload.get("message")?.as_str()?.to_owned()),
        "item_completed" => {
            let item = payload.get("item")?;
            if item.get("type")?.as_str()? != "UserMessage" {
                return None;
            }
            let text: Vec<&str> = item
                .get("content")?
                .as_array()?
                .iter()
                .filter(|part| part.get("type").and_then(|kind| kind.as_str()) == Some("text"))
                .filter_map(|part| part.get("text")?.as_str())
                .collect();
            (!text.is_empty()).then(|| text.join("\n"))
        }
        _ => None,
    }
}

impl AgentAdapter for CodexAdapter {
    fn binary(&self) -> &'static str {
        "codex"
    }

    fn preallocates_session_id(&self) -> bool {
        false
    }

    fn uses_hook_script(&self) -> bool {
        true
    }

    /// Codex's title is a list of items joined by ` | `: a spinner, a status such as `[ ! ] Action
    /// Required`, the conversation's title once it has one, and the project last. An unnamed
    /// conversation is left with the project alone, which says nothing.
    ///
    /// The terminal title reaches here without the spinner it starts with, which leaves the
    /// separator after it at the front.
    fn task_title(&self, raw: &str, project_names: &[String]) -> Option<String> {
        let raw = raw.strip_prefix('|').unwrap_or(raw);
        let mut items: Vec<&str> = raw
            .split(" | ")
            .map(str::trim)
            .filter(|item| {
                !item.is_empty()
                    && !super::is_spinner(item)
                    && !crate::terminal_title::codex_action_required(item)
            })
            .collect();
        if items
            .last()
            .is_some_and(|last| project_names.iter().any(|name| name == last))
        {
            items.pop();
        }
        (!items.is_empty()).then(|| items.join(" | "))
    }

    fn names_project(&self) -> bool {
        true
    }

    /// A line of `session_index.jsonl`, which names every conversation of the account that has a
    /// title, whether Codex generated it or the user renamed it.
    fn indexed_title(&self, line: &str) -> Option<(String, String)> {
        if !line.contains("\"thread_name\"") {
            return None;
        }
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        Some((
            value.get("id")?.as_str()?.to_owned(),
            value.get("thread_name")?.as_str()?.to_owned(),
        ))
    }

    /// What the user typed, from the rollout's event of it: `user_message` in older Codex, an
    /// `item_completed` of a `UserMessage` item in newer. The user-role response items beside them
    /// include the environment context and `AGENTS.md` instructions Codex adds itself.
    ///
    /// TODO(openai/codex#53087, https://github.com/openai/codex/issues/53087): Codex's own thread
    /// titles fail while an MCP server is supplied through `-c`, as Octoboard must, so its Codex
    /// sessions stay unnamed and this first prompt is the only title they get. Once it is fixed,
    /// Codex names threads again and the `session_index.jsonl` title (`indexed_title`) takes over;
    /// then reconsider whether this fallback is still wanted.
    fn prompt_text(&self, line: &str) -> Option<String> {
        if !line.contains("\"user_message\"") && !line.contains("\"UserMessage\"") {
            return None;
        }
        typed_text(&serde_json::from_str(line).ok()?)
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> anyhow::Result<LaunchPlan> {
        let pinned = super::pinned_config_dir(spec, Agent::Codex)?;
        // Unlike Claude Code, Codex refuses a `CODEX_HOME` that does not exist rather than creating
        // it, so an account pointed at a directory the user has yet to create is created here. A
        // resume that needs the directory's conversation has already been refused above.
        if let Some(dir) = pinned {
            std::fs::create_dir_all(dir).map_err(|err| {
                CodedError::raised(
                    error_code::CONFIG_DIR_UNREACHABLE,
                    format!(
                        "could not create the Codex config directory `{}`: {err}",
                        dir.display()
                    ),
                    &[
                        ("agent", Agent::Codex.label()),
                        ("path", &dir.to_string_lossy()),
                    ],
                )
            })?;
        }
        let mut args = Vec::new();

        // `resume` is a subcommand, so it comes before the overrides.
        if let Some(id) = spec.resume_agent_session_id {
            args.push("resume".to_string());
            args.push(id.to_string());
        }

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
        // orchestration unusable. `approve` is the one value that runs them unasked. `auto` leaves
        // the decision to Codex, which asks for a tool carrying no annotations, as Octoboard's own
        // do — and under an approval policy of `never` it does not even ask, it refuses the call
        // outright without sending it. The key is per server, so nothing else the session may need
        // approval for is widened.
        args.push("-c".to_string());
        args.push(format!(
            "mcp_servers.{key}.default_tools_approval_mode=\"approve\""
        ));
        // Codex abandons a tool call at a limit of its own and sends the server nothing when it
        // does, so a call that outlasts it never returns its result — and a call Octoboard holds
        // for the user (`session_request::ANSWER_TIME_LIMIT`) alone outlasts Codex's default.
        // Hence `super::MCP_TOOL_TIMEOUT_SECS`, where the reasoning for the value is.
        args.push("-c".to_string());
        args.push(format!(
            "mcp_servers.{key}.tool_timeout_sec={}",
            super::MCP_TOOL_TIMEOUT_SECS
        ));

        // Codex's own update prompt ("Update now / Skip") would otherwise come up before its trust
        // screen, unrecognised and in the way of it. Off for this launch only.
        args.push("-c".to_string());
        args.push("check_for_update_on_startup=false".to_string());

        args.push("-c".to_string());
        args.push(format!(
            "developer_instructions={}",
            toml_string(&mcp::role::role_description(
                spec.role,
                spec.owner.as_ref(),
                Agent::Codex
            ))
        ));

        // Codex has no effort flag; the effort is reachable only as a config override, so the
        // model goes the same way to keep the two consistent. Both come before the positional
        // task, which Codex takes last. A model that does not advertise the effort is refused by
        // Codex itself, at launch.
        if let Some(model) = spec.model {
            args.push("-c".to_string());
            args.push(format!("model={}", toml_string(model)));
        }
        if let Some(effort) = spec.effort {
            args.push("-c".to_string());
            args.push(format!("model_reasoning_effort={}", toml_string(effort)));
        }

        if let Some(reviewer) = spec.approval_reviewer {
            let value = match reviewer {
                crate::protocol::ApprovalReviewer::User => "user",
                crate::protocol::ApprovalReviewer::AutoReview => "auto_review",
            };
            // Repeated on resume like the other launch overrides; the user's file stays intact.
            args.push("-c".to_string());
            args.push(format!("approvals_reviewer={}", toml_string(value)));
        }

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

/// How long `codex unarchive` may take; it only moves one file.
const UNARCHIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Brings the archived rollout of thread `id` back under `sessions/` by running `codex unarchive`
/// the way a session of the account would run Codex (`config_dir` exported as `CODEX_HOME`, the
/// default account left as the shell has it). A resume refuses an archived thread, so this has to
/// come first.
pub fn unarchive(
    shell_env: &std::collections::HashMap<String, String>,
    config_dir: Option<&Path>,
    id: &str,
) -> anyhow::Result<()> {
    let failed = |detail: String| {
        CodedError::raised(
            error_code::CODEX_UNARCHIVE_FAILED,
            format!("`codex unarchive` failed: {detail}"),
            &[("detail", &detail)],
        )
    };
    let mut command =
        super::account_command(Agent::Codex, shell_env, config_dir, &["unarchive", id]).map_err(
            |binary| {
                CodedError::raised(
                    error_code::BINARY_NOT_FOUND,
                    format!("`{binary}` was not found on PATH in the shell environment"),
                    &[("binary", binary)],
                )
            },
        )?;
    let output = crate::subprocess::run_with_timeout(&mut command, UNARCHIVE_TIMEOUT)
        .map_err(|err| failed(err.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim().lines().next_back().unwrap_or_default();
    Err(failed(if message.is_empty() {
        output.status.to_string()
    } else {
        message.to_owned()
    }))
}

/// Whether this user's Codex resolves approval requests without ever showing a dialog.
///
/// The account override takes precedence; otherwise read the user's own `config.toml`, never
/// written to. The setting's shape is not pinned down (it has been seen as a plain value and as a
/// table), so the whole value is searched for the
/// auto-review marker rather than one key path being assumed. Unreadable or absent means "the user
/// will be asked", which is the safe way to be wrong: a hand raised needlessly is visible, while
/// one never raised leaves a session looking busy while it waits.
fn resolves_approvals_itself(spec: &LaunchSpec<'_>, pinned: Option<&Path>) -> bool {
    if let Some(reviewer) = spec.approval_reviewer {
        return reviewer == crate::protocol::ApprovalReviewer::AutoReview;
    }
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
    super::default_account_dir(Agent::Codex, spec.shell_env)
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

    #[test]
    fn terminal_titles_keep_only_the_task_between_the_spinner_and_the_project() {
        let names = ["proj".to_owned()];
        let task = |raw| CodexAdapter.task_title(raw, &names);
        for raw in [
            "proj",
            "⠧ ⠧ | proj",
            "⠙ | proj",
            "| proj",
            "[ ! ] Action Required | proj",
            "[ . ] Action Required | proj",
            "",
        ] {
            assert_eq!(task(raw), None, "{raw}");
        }
        assert_eq!(
            task("Explain pseudo terminal | proj").as_deref(),
            Some("Explain pseudo terminal")
        );
        assert_eq!(
            task("⠙ | Explain pseudo terminal | proj").as_deref(),
            Some("Explain pseudo terminal")
        );
        assert_eq!(task("Fix a | b").as_deref(), Some("Fix a | b"));
        assert_eq!(
            task("[ x ] Fix it | proj").as_deref(),
            Some("[ x ] Fix it"),
            "only the known status items are dropped"
        );
    }

    fn overrides(args: &[String]) -> Vec<String> {
        args.windows(2)
            .filter(|pair| pair[0] == "-c")
            .map(|pair| pair[1].clone())
            .collect()
    }

    /// With no model or effort set, neither override is passed, so Codex keeps its own choice.
    #[test]
    fn no_model_or_effort_adds_no_overrides() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        assert!(!overrides(&plan.args)
            .iter()
            .any(|override_| override_.starts_with("model")));
    }

    #[test]
    fn a_model_and_effort_are_config_overrides_ahead_of_the_task() {
        let mut fixture = spec_fixture();
        fixture.model = Some("gpt-5".to_string());
        fixture.effort = Some("high".to_string());
        let mut spec = fixture.spec();
        spec.task = Some("do it");
        let plan = CodexAdapter.plan(&spec).expect("plan");
        let overrides = overrides(&plan.args);
        assert!(overrides.contains(&"model=\"gpt-5\"".to_string()));
        assert!(overrides.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert_eq!(plan.args.last().map(String::as_str), Some("do it"));
    }

    /// Project trust is Codex's own to record when its confirmation is pressed, so it is never
    /// passed for one launch; the update prompt, which would come up before that confirmation, is
    /// turned off for the launch.
    #[test]
    fn passes_no_project_trust_and_no_update_check() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        let overrides = overrides(&plan.args);
        assert!(
            !overrides
                .iter()
                .any(|override_| override_.starts_with("projects")),
            "{overrides:?}"
        );
        assert!(overrides.contains(&"check_for_update_on_startup=false".to_string()));
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
        assert!(overrides.contains(
            &"mcp_servers.octoboard.default_tools_approval_mode=\"approve\"".to_string()
        ));
    }

    /// Codex abandons a tool call at a limit of its own, silently, and its default would end a long
    /// Octoboard call first (the "Codex" section of `docs/agent-cli-reference.md` has the value);
    /// the override puts it above Octoboard's forwarding limit, for this server only.
    #[test]
    fn the_servers_tool_timeout_sits_above_octoboards_own_call_limit() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        let overrides = overrides(&plan.args);
        assert!(overrides.contains(&"mcp_servers.octoboard.tool_timeout_sec=660".to_string()));
        assert!(!overrides.iter().any(|o| o.starts_with("tool_timeout_sec")));
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
            toml_string(&mcp::role::role_description(
                fixture.role,
                fixture.owner.as_ref(),
                Agent::Codex
            ))
        ))));
        assert!(!overrides.iter().any(|o| o.starts_with("instructions=")));
    }

    #[test]
    fn account_reviewer_overrides_are_repeated_on_resume_without_changing_the_users_config() {
        use crate::protocol::ApprovalReviewer;
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("reviewer-home");
        std::fs::create_dir_all(&home).unwrap();
        let config = home.join("config.toml");
        let original = "approvals_reviewer = \"auto_review\"\n";
        std::fs::write(&config, original).unwrap();
        fixture.config_dir = Some(home);
        for (reviewer, text, resolves) in [
            (ApprovalReviewer::User, "user", false),
            (ApprovalReviewer::AutoReview, "auto_review", true),
        ] {
            for resume in [None, Some("conversation-1")] {
                let mut spec = fixture.spec();
                spec.approval_reviewer = Some(reviewer);
                spec.resume_agent_session_id = resume;
                spec.task = Some("Initial prompt");
                let plan = CodexAdapter.plan(&spec).unwrap();
                assert_eq!(plan.resolves_approvals_itself, resolves);
                assert!(overrides(&plan.args).contains(&format!("approvals_reviewer=\"{text}\"")));
                assert!(!plan
                    .args
                    .iter()
                    .any(|arg| arg.contains("approval_policy") || arg.contains("sandbox_mode")));
                if resume.is_none() {
                    assert_eq!(plan.args.last().unwrap(), "Initial prompt");
                }
                assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
            }
        }
        let plan = CodexAdapter.plan(&fixture.spec()).unwrap();
        assert!(plan.resolves_approvals_itself);
        assert!(!overrides(&plan.args)
            .iter()
            .any(|arg| arg.starts_with("approvals_reviewer=")));
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
    fn a_pinned_codex_home_that_has_gone_refuses_resuming_a_conversation() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some("agent-side-id");
        let err = CodexAdapter.plan(&spec).err().expect("refused");
        let message = err.to_string();
        assert!(message.contains("Codex"), "{message}");
        assert!(message.contains("missing"), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }

    /// A fresh session, and one that never had a turn, launch into a vanished pinned directory
    /// rather than being refused, and the directory is created for them: Codex itself exits on a
    /// `CODEX_HOME` that does not exist.
    #[test]
    fn a_pinned_codex_home_that_has_gone_is_created_with_no_conversation_to_resume() {
        let mut fixture = spec_fixture();
        let dir = fixture.scratch.join("missing").join("nested");
        fixture.config_dir = Some(dir.clone());
        CodexAdapter.plan(&fixture.spec()).expect("not refused");
        assert!(dir.is_dir());
    }

    #[test]
    fn never_pre_allocates_a_session_id() {
        let fixture = spec_fixture();
        let plan = CodexAdapter.plan(&fixture.spec()).expect("plan");
        assert!(!CodexAdapter.preallocates_session_id());
        assert!(!plan.args.iter().any(|arg| arg.contains("session-id")));
    }
}
