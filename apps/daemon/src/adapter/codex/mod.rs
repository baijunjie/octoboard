//! Codex adapter. Configuration is injected through repeated per-invocation `-c` overrides on the
//! terminal's own command line, without changing the user's configuration files. A resume repeats
//! them, the permission preset included.
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
//! A permission preset selects the built-in permission profile, approval policy and reviewer.
//! Without a preset, Codex uses account configuration on a fresh launch and saved thread
//! permissions on a resume.
//!
//! The effective `approvals_reviewer` changes what Octoboard may promise: with it set to auto review,
//! the `PermissionRequest` hook fires for requests resolved without showing a modal. The adapter
//! resolves the account override or reads the user's setting so those events do not raise a hand.
//! Actual action prompts are observed separately from the terminal activity title, since automatic
//! review does not rule out manual approvals.
//!
//! Codex allocates its own thread ids, so the conversation id arrives through the hooks; thread id
//! and session id match.

pub(crate) mod accounts;
mod app_server;
mod claude_compatibility;
mod claude_mcp;
mod compatibility;
mod config;
mod conversation;
mod hooks;
mod login;
mod mcp_auth;
mod mcp_inventory;
pub(crate) mod models;
mod native_binary;
pub(crate) mod quota;
pub(crate) mod records;
mod reset_credit;
mod rollout_files;
pub(crate) mod rollout_watch;
pub(crate) mod screen;
mod shared_config;
mod switch_continue;
#[cfg(test)]
mod tests;
pub(crate) mod threads;
mod unarchive;

use serde_json::json;

use super::files::canonical_project;
use super::{AgentProvider, LaunchPlan, LaunchSpec, COMMAND_HOOK_TIMEOUT_SECS};
use crate::platform::shell::{self, ResolvedBinary};
use crate::protocol::{error_code, AgentId, CodedError};
use crate::providers::{codex, ProviderDescriptor};
use crate::session::screen::ScreenReader;
use crate::session_tools;
use claude_compatibility::reserved_env;
use config::{model_migrations, project_server_names, read_config, server_names, ConfigFile};

/// The permission preset held under `approval_reviewer`. Unset leaves permission selection to
/// Codex's native launch or resume behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PermissionMode {
    User,
    AutoReview,
    FullAccess,
}

impl PermissionMode {
    fn of(settings: &crate::providers::ProviderSettings) -> Option<Self> {
        match settings.choice("approval_reviewer")? {
            "user" => Some(Self::User),
            "auto_review" => Some(Self::AutoReview),
            "full_access" => Some(Self::FullAccess),
            _ => None,
        }
    }
}

/// The override that makes Codex keep MCP OAuth tokens in `$CODEX_HOME/.credentials.json` rather
/// than its default store, passed on every Codex start Octoboard makes for a non-default account
/// that could read or write those tokens, and on none for the default account.
///
/// On macOS the default store is the keychain, under a key computed from the server's name and URL
/// alone, not from `CODEX_HOME`, so without it every account would share one token per server —
/// one identity at the server, with uncoordinated refreshes. It cannot be set in the account's
/// `config.toml`, which a sharing account has in common with the default account (and which
/// Octoboard never writes), so it goes per launch. The default account keeps Codex's default store,
/// matching `codex` run in a terminal.
pub(crate) const MCP_FILE_CREDENTIALS_STORE: &str = "mcp_oauth_credentials_store=\"file\"";

/// Both phases of Codex's blinking activity title identify a visible user interaction.
pub(crate) fn action_required_title(title: &str) -> bool {
    matches!(
        title.split(" | ").next().map(str::trim),
        Some("[ ! ] Action Required" | "[ . ] Action Required")
    )
}

/// The events Octoboard's session states are derived from. `Interrupt` is included because it is
/// mutually exclusive with `Stop` and is the only signal of a cancelled turn Claude Code, Codex or
/// Grok Build gives. `SubagentStart` / `SubagentStop` are deliberately not injected: they are noise
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

/// Timeout registered for `event`'s hook, in seconds. `Interrupt` keeps 3 s on every host: Codex
/// is recorded to clamp it to 3 s and warn visibly about a longer one (from an earlier comment, not
/// verified), and it is never the cold first hook of a launch. The rest take the host's allowance.
fn hook_timeout_secs(event: &str) -> u32 {
    if event == "Interrupt" {
        COMMAND_HOOK_TIMEOUT_SECS.min(3)
    } else {
        COMMAND_HOOK_TIMEOUT_SECS
    }
}

pub struct CodexProvider;

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

impl AgentProvider for CodexProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::CODEX.descriptor()
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        &screen::CodexScreen
    }

    fn hooks(&self) -> &'static dyn crate::hooks::HookInterpreter {
        &hooks::CodexHooks
    }

    fn accounts(&self) -> Option<&'static dyn crate::accounts::AccountOps> {
        Some(&accounts::CodexAccounts)
    }

    fn catalogue(&self) -> &'static dyn crate::agents::models::ModelCatalogue {
        &models::CodexCatalogue
    }

    fn compatibility_source(&self) -> Option<&'static dyn super::CompatibilitySource> {
        Some(&compatibility::CodexCompatibility)
    }

    fn records(&self) -> &'static dyn crate::records::RecordStore {
        &records::CodexRecords
    }

    /// The standalone install is what Codex's own installer leaves under `~/.codex/packages`; the
    /// package is `@openai/codex`, and Homebrew has a formula and a cask, both named `codex`.
    /// `codex update` updates a standalone install.
    fn cli_packaging(&self) -> &'static super::CliPackaging {
        &super::CliPackaging {
            native_layouts: &[&[".codex", "packages", "standalone"]],
            native_update: &["update"],
            native_latest: None,
            npm_package: Some("@openai/codex"),
            homebrew_formula: Some("codex"),
            homebrew_cask: Some("codex"),
        }
    }

    fn native_binary(&self, binary: &ResolvedBinary) -> anyhow::Result<Option<ResolvedBinary>> {
        native_binary::of(binary)
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
        let pinned = super::pinned_config_dir(spec, AgentId::CODEX)?;
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
                        ("agent", AgentId::CODEX.label()),
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
        args.push("--dangerously-bypass-hook-trust".to_string());

        // Codex runs the command through the host's own shell, whose language it is written in.
        let command = shell::script_command(spec.hook_script, shell::hook_dialect());
        for event in HOOK_EVENTS {
            args.push("-c".to_string());
            args.push(format!(
                "hooks.{event}=[{{hooks=[{{type=\"command\",command={},timeout={},async=true}}]}}]",
                toml_string(&command),
                hook_timeout_secs(event)
            ));
        }

        let (mcp_command, mcp_args) = super::mcp_server_command(spec);
        let key = session_tools::catalogue::SERVER_KEY;
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
        // A non-default account keeps its MCP OAuth tokens in its own home. See
        // `MCP_FILE_CREDENTIALS_STORE` for why the default account is left alone.
        if pinned.is_some() {
            args.push("-c".to_string());
            args.push(MCP_FILE_CREDENTIALS_STORE.to_string());
        }

        // Read from the launch environment rather than the daemon's: the daemon's own `CODEX_HOME`
        // may belong to an agent session it was started from.
        let home = super::account_dir(AgentId::CODEX, pinned, spec.shell_env);
        let config = read_config(&home);
        // Set only for a pinned directory; otherwise the shell snapshot's own value, if any,
        // stands.
        let mut env = pinned
            .map(|dir| {
                vec![(
                    codex::LAYOUT.config_dir_var.to_string(),
                    dir.to_string_lossy().into_owned(),
                )]
            })
            .unwrap_or_default();
        // The compatibility source's MCP servers (Claude Code's), for the session's working
        // directory. A server of the same name in the account's own `config.toml` or the project's
        // `.codex/config.toml` stays as it is, and an unreadable one leaves nothing to compare
        // with, so then none is added.
        if let Some(source) = spec.compatibility_source(AgentId::CLAUDE) {
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
            let servers = claude_mcp::codex_servers(
                source.mcp_servers(spec.shell_env, spec.cwd),
                spec.shell_env,
                &env,
                &reserved_env,
                &taken,
            );
            for server_override in servers.overrides {
                args.push("-c".to_string());
                args.push(server_override);
            }
            env.extend(servers.env);
        }

        // Codex's own update prompt ("Update now / Skip") would otherwise come up before its trust
        // screen, unrecognised and in the way of it. Off for this launch only.
        args.push("-c".to_string());
        args.push("check_for_update_on_startup=false".to_string());

        args.push("-c".to_string());
        args.push(format!("tui.terminal_title={TERMINAL_TITLE}"));
        if spec.resume_agent_session_id.is_some() {
            args.push("-c".to_string());
            args.push("tui.resume_cwd=\"current\"".to_string());
        }
        args.push("-c".to_string());
        args.push("notice.hide_rate_limit_model_nudge=true".to_string());
        if let Some(upgrades) = model_migrations(&home) {
            args.push("-c".to_string());
            args.push(format!("notice.model_migrations={upgrades}"));
        }

        let mut instructions = session_tools::role::role_description(
            spec.kind,
            spec.owner.as_ref(),
            AgentId::CODEX,
            &spec.role_options(),
        );
        if let Some(source) = spec.compatibility_source(AgentId::CLAUDE) {
            instructions.push_str("\n\n");
            instructions.push_str(&source.instructions(AgentId::CODEX.label(), spec.shell_env));
        }
        args.push("-c".to_string());
        args.push(format!(
            "developer_instructions={}",
            toml_string(&instructions)
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

        if let Some(mode) = PermissionMode::of(spec.settings) {
            let (profile, policy, reviewer) = match mode {
                PermissionMode::User => (":workspace", "on-request", "user"),
                PermissionMode::AutoReview => (":workspace", "on-request", "auto_review"),
                PermissionMode::FullAccess => (":danger-full-access", "never", "user"),
            };
            // Built-in profiles keep a preset from inheriting legacy writable roots or network
            // access. Explicit per-launch choices also replace a resumed thread's saved choices.
            for (key, value) in [
                ("default_permissions", profile),
                ("approval_policy", policy),
                ("approvals_reviewer", reviewer),
            ] {
                args.push("-c".to_string());
                args.push(format!("{key}={}", toml_string(value)));
            }
        }

        if spec.resume_agent_session_id.is_none() {
            if let Some(task) = spec.task {
                args.push(task.to_string());
            }
        }

        Ok(LaunchPlan {
            args,
            env,
            resolves_approvals_itself: resolves_approvals_itself(spec, &config),
            ..LaunchPlan::default()
        })
    }
}

/// Whether Codex handles permission-request hooks without asking the user.
///
/// The account override takes precedence; otherwise read the user's own `config.toml`, never
/// written to. The setting's shape is not pinned down (it has been seen as a plain value and as a
/// table), so the whole value is searched for the
/// auto-review marker rather than one key path being assumed. Unreadable or absent means "the user
/// will be asked", which is the safe way to be wrong: a hand raised needlessly is visible, while
/// one never raised leaves a session looking busy while it waits.
fn resolves_approvals_itself(spec: &LaunchSpec<'_>, config: &ConfigFile) -> bool {
    if let Some(mode) = PermissionMode::of(spec.settings) {
        return mode != PermissionMode::User;
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
