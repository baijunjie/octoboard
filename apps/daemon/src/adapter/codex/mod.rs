//! Codex adapter. Configuration is injected through repeated per-invocation `-c` overrides,
//! without changing the user's configuration files. Verified releases run those overrides in a
//! private app server and explicitly connect the TUI to it. Rendering and startup notices need client-side
//! overrides, while hooks and model execution belong to the server. Remote resumes apply an
//! explicit approval reviewer through the server because the TUI refuses permission overrides;
//! Codex checkpoints that reviewer in the thread before the TUI attaches.
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
//! Every launch also overrides four things, for this launch only. Three would otherwise put a
//! screen in front of the session that no hook or terminal title announces, and that a message's
//! paste and Enter would answer destructively: a resume asks which of two working directories to
//! use (`tui.resume_cwd` starts it in the current one); the model-migration prompt is shown for a
//! model that has an upgrade (`model_migrations` suppresses it for every model the models cache
//! lists one for); and the rate-limit "switch model" nudge appears after a turn, once a limit is
//! near (hidden). The fourth restores the start signal rather than removing a screen: the terminal
//! title is pinned to a fixed set of items (`TERMINAL_TITLE`), since the trust watch ends at the
//! first title and a configuration that draws none (a project's own `[tui] terminal_title = []`, or
//! items that are empty at the start) would hold messages for good; overrides outrank project
//! configuration.
//!
//! Two kinds of trust are involved, and they are handled differently:
//!
//! - **Project trust** is not passed. In a git repository Codex has not been told to trust, it
//!   shows its own "Trust this folder?" confirmation, which Octoboard presses under the user's
//!   permission (`crate::session::trust`); Codex then records `trust_level = "trusted"` for the
//!   repository root in its own `config.toml`, so a later launch is trusted from the start. A
//!   per-launch `projects` override would be saved nowhere, and would mark the folder trusted on
//!   the user's behalf.
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
//! Codex allocates its own thread ids. A private server exposes its loaded thread before the first
//! submission, allowing independent messages even while the user is composing the first prompt.
//! The persisted conversation id still arrives through the hooks; thread id and session id match.

mod claude_compatibility;
mod claude_mcp;
mod config;
mod private_server;
#[cfg(test)]
mod tests;
pub(crate) mod transport;
mod unarchive;

use serde_json::json;

use super::files::{canonical_project, read_regular_text, SMALL_FILE_LIMIT};
use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};
use crate::agents::layout;
use crate::protocol::{error_code, Agent, CodedError};
use crate::session_tools;
use claude_compatibility::{claude_compatibility_instructions, reserved_env};
use config::{model_migrations, project_server_names, read_config, server_names, ConfigFile};
use private_server::private_server_supported;

pub use private_server::PrivateServerPlan;
pub use unarchive::unarchive;

/// Both phases of Codex's blinking activity title identify a visible user interaction.
pub(crate) fn action_required_title(title: &str) -> bool {
    matches!(
        title.split(" | ").next().map(str::trim),
        Some("[ ! ] Action Required" | "[ . ] Action Required")
    )
}

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

    fn installed_version(&self, binary: &str) -> Option<super::AgentVersion> {
        let binary = std::fs::canonicalize(binary).ok()?;
        for directory in binary.ancestors().skip(1).take(4) {
            for name in ["codex-package.json", "package.json"] {
                let Ok(text) = read_regular_text(&directory.join(name), SMALL_FILE_LIMIT) else {
                    continue;
                };
                let value: serde_json::Value = serde_json::from_str(&text).ok()?;
                if name == "package.json"
                    && value.get("name").and_then(|value| value.as_str()) != Some("@openai/codex")
                {
                    continue;
                }
                return super::parse_version(value.get("version")?.as_str()?);
            }
        }
        None
    }

    /// Codex's title is a list of items joined by ` | `: a spinner, a status such as `[ ! ] Action
    /// Required`, the conversation's title once it has one, and the project last. An unnamed
    /// conversation is left with the project alone, which says nothing. Codex cuts a project name
    /// longer than 24 graphemes to its first 21 and `...`, as it does a console's directory,
    /// named by the console's id.
    ///
    /// The terminal title reaches here without the spinner it starts with, which leaves the
    /// separator after it at the front.
    fn task_title(&self, raw: &str, project_names: &[String]) -> Option<String> {
        let raw = raw.strip_prefix('|').unwrap_or(raw);
        let mut items: Vec<&str> = raw
            .split(" | ")
            .map(str::trim)
            .filter(|item| {
                !item.is_empty() && !super::is_spinner(item) && !action_required_title(item)
            })
            .collect();
        if items
            .last()
            .is_some_and(|last| project_names.iter().any(|name| is_project_item(last, name)))
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
    /// TODO(openai/codex#53087, <https://github.com/openai/codex/issues/53087>): Codex's own thread
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
        let mut server_args = Vec::new();

        // `resume` is a subcommand, so it comes before the overrides.
        if let Some(id) = spec.resume_agent_session_id {
            args.push("resume".to_string());
            args.push(id.to_string());
        }
        // A remote TUI cannot infer the workspace from its process's working directory.
        args.extend(["--cd".to_string(), spec.cwd.to_string_lossy().into_owned()]);

        let command = spec.hook_script.to_string_lossy().into_owned();
        for event in HOOK_EVENTS {
            server_args.push("-c".to_string());
            server_args.push(format!(
                "hooks.{event}=[{{hooks=[{{type=\"command\",command={},timeout={},async=true}}]}}]",
                toml_string(&command),
                HOOK_TIMEOUT_SECS
            ));
        }
        args.push("--dangerously-bypass-hook-trust".to_string());

        let (mcp_command, mcp_args) = super::mcp_server_command(spec);
        let key = session_tools::catalogue::SERVER_KEY;
        server_args.push("-c".to_string());
        server_args.push(format!(
            "mcp_servers.{key}.command={}",
            toml_string(&mcp_command)
        ));
        server_args.push("-c".to_string());
        server_args.push(format!(
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
        server_args.push("-c".to_string());
        server_args.push(format!(
            "mcp_servers.{key}.default_tools_approval_mode=\"approve\""
        ));
        // Codex abandons a tool call at a limit of its own and sends the server nothing when it
        // does, so a call that outlasts it never returns its result — and a call Octoboard holds
        // for the user (`session_request::ANSWER_TIME_LIMIT`) alone outlasts Codex's default.
        // Hence `super::MCP_TOOL_TIMEOUT_SECS`, where the reasoning for the value is.
        server_args.push("-c".to_string());
        server_args.push(format!(
            "mcp_servers.{key}.tool_timeout_sec={}",
            super::MCP_TOOL_TIMEOUT_SECS
        ));

        // Read from the launch environment rather than the daemon's: the daemon's own `CODEX_HOME`
        // may belong to an agent session it was started from.
        let home = super::account_dir(Agent::Codex, pinned, spec.shell_env);
        let config = read_config(&home);
        // Set only for a pinned directory; otherwise the shell snapshot's own value, if any,
        // stands.
        let mut env = pinned
            .map(|dir| {
                vec![(
                    layout::CODEX.config_dir_var.to_string(),
                    dir.to_string_lossy().into_owned(),
                )]
            })
            .unwrap_or_default();
        // Claude Code's MCP servers, for the session's working directory. A server of the same name
        // in the account's own `config.toml` or the project's `.codex/config.toml` stays as it is,
        // and an unreadable one leaves nothing to compare with, so then none is added.
        if spec.claude_compatibility {
            let project_names = project_server_names(&canonical_project(spec.cwd), &home);
            let taken = |name: &str| {
                project_names
                    .as_ref()
                    .is_none_or(|names| names.contains(name))
                    || match &config {
                        ConfigFile::Absent => false,
                        ConfigFile::Unreadable => true,
                        ConfigFile::Parsed(parsed) => server_names(parsed).contains(name),
                    }
            };
            let servers =
                claude_mcp::codex_servers(spec.shell_env, &env, &reserved_env, spec.cwd, &taken);
            for server_override in servers.overrides {
                server_args.push("-c".to_string());
                server_args.push(server_override);
            }
            env.extend(servers.env);
        }

        // Codex's own update prompt ("Update now / Skip") would otherwise come up before its trust
        // screen, unrecognised and in the way of it. Off for this launch only.
        server_args.push("-c".to_string());
        server_args.push("check_for_update_on_startup=false".to_string());

        server_args.push("-c".to_string());
        server_args.push(format!("tui.terminal_title={TERMINAL_TITLE}"));
        if spec.resume_agent_session_id.is_some() {
            server_args.push("-c".to_string());
            server_args.push("tui.resume_cwd=\"current\"".to_string());
        }
        server_args.push("-c".to_string());
        server_args.push("notice.hide_rate_limit_model_nudge=true".to_string());
        if let Some(upgrades) = model_migrations(&home) {
            server_args.push("-c".to_string());
            server_args.push(format!("notice.model_migrations={upgrades}"));
        }

        let mut instructions = session_tools::role::role_description(
            spec.role,
            spec.owner.as_ref(),
            Agent::Codex,
            &spec.role_options(),
        );
        if spec.claude_compatibility {
            instructions.push_str("\n\n");
            instructions.push_str(&claude_compatibility_instructions(spec.shell_env));
        }
        server_args.push("-c".to_string());
        server_args.push(format!(
            "developer_instructions={}",
            toml_string(&instructions)
        ));

        // Codex has no effort flag; the effort is reachable only as a config override, so the
        // model goes the same way to keep the two consistent. Both come before the positional
        // task, which Codex takes last. A model that does not advertise the effort is refused by
        // Codex itself, at launch.
        if let Some(model) = spec.model {
            server_args.push("-c".to_string());
            server_args.push(format!("model={}", toml_string(model)));
        }
        if let Some(effort) = spec.effort {
            server_args.push("-c".to_string());
            server_args.push(format!("model_reasoning_effort={}", toml_string(effort)));
        }

        // Remote resumes reject explicit permission overrides in the TUI. Its server receives
        // the reviewer below; the client still needs the other overrides for local UI settings.
        let resume_tui_args = spec.resume_agent_session_id.map(|_| {
            let mut tui_args = args.clone();
            tui_args.extend(server_args.iter().cloned());
            tui_args
        });

        if let Some(reviewer) = spec.approval_reviewer {
            let value = match reviewer {
                crate::protocol::ApprovalReviewer::User => "user",
                crate::protocol::ApprovalReviewer::AutoReview => "auto_review",
            };
            // Repeated on resume like the other launch overrides; the user's file stays intact.
            server_args.push("-c".to_string());
            server_args.push(format!("approvals_reviewer={}", toml_string(value)));
        }

        args.extend(server_args.iter().cloned());

        if spec.resume_agent_session_id.is_none() {
            if let Some(task) = spec.task {
                args.push(task.to_string());
            }
        }

        let codex_server =
            private_server_supported(spec.agent_version).then(|| PrivateServerPlan {
                args: server_args,
                // Explicit --remote selects the server even with -c. Omitting the client overrides
                // would lose its terminal title, resume cwd and startup-notice settings.
                tui_args: resume_tui_args.unwrap_or_else(|| args.clone()),
                hook_command: command,
                resume_thread: spec.resume_agent_session_id.map(str::to_string),
                resume_reviewer: spec.approval_reviewer,
            });
        Ok(LaunchPlan {
            args,
            codex_server,
            env,
            resolves_approvals_itself: resolves_approvals_itself(spec, &config),
            ..LaunchPlan::default()
        })
    }
}

/// Whether this user's Codex resolves approval requests without ever showing a dialog.
///
/// The account override takes precedence; otherwise read the user's own `config.toml`, never
/// written to. The setting's shape is not pinned down (it has been seen as a plain value and as a
/// table), so the whole value is searched for the
/// auto-review marker rather than one key path being assumed. Unreadable or absent means "the user
/// will be asked", which is the safe way to be wrong: a hand raised needlessly is visible, while
/// one never raised leaves a session looking busy while it waits.
fn resolves_approvals_itself(spec: &LaunchSpec<'_>, config: &ConfigFile) -> bool {
    if let Some(reviewer) = spec.approval_reviewer {
        return reviewer == crate::protocol::ApprovalReviewer::AutoReview;
    }
    const AUTO_REVIEW: &str = "auto_review";
    let ConfigFile::Parsed(parsed) = config else {
        return false;
    };
    parsed
        .get("approvals_reviewer")
        .is_some_and(|value| mentions(value, AUTO_REVIEW))
}

/// The terminal title items every launch is given, in the order Codex's default title draws them.
/// `thread-title` is left out: until a thread is named it falls back to the thread id.
const TERMINAL_TITLE: &str = "[\"activity\",\"thread-name\",\"project-name\"]";

/// Whether `wanted` appears as a string anywhere inside this value.
fn mentions(value: &toml::Value, wanted: &str) -> bool {
    match value {
        toml::Value::String(text) => text == wanted,
        toml::Value::Array(items) => items.iter().any(|item| mentions(item, wanted)),
        toml::Value::Table(table) => table.values().any(|item| mentions(item, wanted)),
        _ => false,
    }
}

/// Whether a title item is the project `name`, whole or cut short with `...`. Any shorter prefix is
/// taken, rather than exactly the first 21 graphemes Codex keeps, which would take grapheme
/// segmentation to count.
fn is_project_item(item: &str, name: &str) -> bool {
    item == name
        || item.strip_suffix("...").is_some_and(|head| {
            !head.is_empty() && name.len() > head.len() && name.starts_with(head)
        })
}

/// A TOML basic string. JSON string escaping is a subset of TOML's, so serde_json produces a valid
/// one and spares us a second escaping implementation.
pub(super) fn toml_string(value: &str) -> String {
    json!(value).to_string()
}
