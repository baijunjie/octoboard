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
//! Codex allocates its own thread ids. A private server exposes its loaded thread before the first
//! submission, allowing independent messages even while the user is composing the first prompt.
//! The persisted conversation id still arrives through the hooks; thread id and session id match.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::json;

use super::claude_mcp::Reserved;
use super::files::{
    canonical_project, is_agent_config_dir, read_bounded_text, read_regular_text, SMALL_FILE_LIMIT,
};
use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};
use crate::line_file::JSON_FILE_LIMIT;
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

pub struct PrivateServerPlan {
    pub args: Vec<String>,
    pub tui_args: Vec<String>,
    pub hook_command: String,
    pub resume_thread: Option<String>,
    pub resume_reviewer: Option<crate::protocol::ApprovalReviewer>,
}

fn private_server_supported(version: Option<super::AgentVersion>) -> bool {
    version == Some((0, 163, 0))
}

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
                let mut parts = value.get("version")?.as_str()?.split('.');
                let version = (
                    parts.next()?.parse().ok()?,
                    parts.next()?.parse().ok()?,
                    parts.next()?.parse().ok()?,
                );
                return parts.next().is_none().then_some(version);
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
                !item.is_empty()
                    && !super::is_spinner(item)
                    && !crate::terminal_title::codex_action_required(item)
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
        let key = mcp::SERVER_KEY;
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

        let home = codex_home(spec, pinned);
        let config = read_config(&home);
        // Set only for a pinned directory; otherwise the shell snapshot's own value, if any,
        // stands.
        let mut env = pinned
            .map(|dir| vec![("CODEX_HOME".to_string(), dir.to_string_lossy().into_owned())])
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
            let servers = super::claude_mcp::codex_servers(
                spec.shell_env,
                &env,
                &reserved_env,
                spec.cwd,
                &taken,
            );
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

        let mut instructions = mcp::role::role_description(
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
    let output = crate::subprocess::run_agent_with_timeout(&mut command, UNARCHIVE_TIMEOUT)
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

/// The `notice.model_migrations` table for the models cache in `codex_home`: every model that has
/// an upgrade target, mapped to it, so that whichever model ends up in effect, Codex's "try the new
/// model" prompt is not shown. `None` when the cache is missing, unreadable within its size bound
/// or lists no upgrade. Read from the file, since asking Codex would start a process on every
/// launch.
fn model_migrations(codex_home: &Path) -> Option<String> {
    let text = read_regular_text(
        &codex_home.join("models_cache.json"),
        JSON_FILE_LIMIT as usize,
    )
    .ok()?;
    upgrade_table(&text)
}

fn upgrade_table(cache: &str) -> Option<String> {
    let cache: serde_json::Value = serde_json::from_str(cache).ok()?;
    // A key repeated in the table would make the whole override a TOML error.
    let mut seen = std::collections::HashSet::new();
    let entries: Vec<String> = cache
        .get("models")?
        .as_array()?
        .iter()
        .filter_map(|model| {
            let from = model.get("slug")?.as_str()?;
            let to = model.get("upgrade")?.get("model")?.as_str()?;
            seen.insert(from)
                .then(|| format!("{}={}", toml_string(from), toml_string(to)))
        })
        .collect();
    (!entries.is_empty()).then(|| format!("{{{}}}", entries.join(",")))
}

/// What the user's `config.toml` in the Codex home in effect came to, for each reader to take its
/// own safe side of.
enum ConfigFile {
    Absent,
    /// It exists but cannot be read within its size bound, or parsed.
    Unreadable,
    Parsed(toml::Table),
}

fn read_config(codex_home: &Path) -> ConfigFile {
    let text = match read_regular_text(&codex_home.join("config.toml"), SMALL_FILE_LIMIT) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return ConfigFile::Absent,
        Err(_) => return ConfigFile::Unreadable,
    };
    // A `Table`, not a `Value`: in `toml` 1.x parsing into `Value` expects a bare value rather
    // than a whole document, and silently fails on the first key.
    match text.parse::<toml::Table>() {
        Ok(parsed) => ConfigFile::Parsed(parsed),
        Err(_) => ConfigFile::Unreadable,
    }
}

/// The names of the MCP servers a parsed `config.toml` defines.
fn server_names(config: &toml::Table) -> BTreeSet<String> {
    config
        .get("mcp_servers")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|servers| servers.keys().cloned())
        .collect()
}

/// The names of the MCP servers the project's `.codex/config.toml` files define, layered over the
/// account's own: the one in the session's working directory and in each directory above it up to
/// the repository root (the nearest one holding `.git`; the working directory alone outside a
/// repository). `None` when one of them exists and cannot be read or parsed. `project` is the
/// canonical working directory; a `.codex` that is `codex_home`, as in a home directory opened as
/// the project, holds the user's own `config.toml` rather than the project's.
fn project_server_names(project: &Path, codex_home: &Path) -> Option<BTreeSet<String>> {
    let mut dirs = Vec::new();
    let mut found = false;
    for dir in project.ancestors() {
        dirs.push(dir);
        if dir.join(".git").exists() {
            found = true;
            break;
        }
    }
    if !found {
        dirs.truncate(1);
    }
    let mut names = BTreeSet::new();
    for dir in dirs {
        let path = dir.join(".codex/config.toml");
        let read = if is_agent_config_dir(&dir.join(".codex"), codex_home) {
            read_regular_text(&path, SMALL_FILE_LIMIT)
        } else {
            read_bounded_text(&path, project, SMALL_FILE_LIMIT)
        };
        let text = match read {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "could not read a project Codex config");
                return None;
            }
        };
        match text.parse::<toml::Table>() {
            Ok(parsed) => names.extend(server_names(&parsed)),
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "could not parse a project Codex config");
                return None;
            }
        }
    }
    Some(names)
}

/// How a Claude Code MCP server's `env` may treat a variable name. `OCTOBOARD_*` is Octoboard's own
/// namespace (its header variables are in `plan.env`), the session variables and `GIT_CONFIG_*` are
/// set after `plan.env` in project sessions, and `TERM` is set last, so a server naming one is
/// refused. What steers Codex itself is `Held`: its authentication and behaviour (`CODEX_*`,
/// `OPENAI_*`), logging (`RUST_LOG`, `RUST_BACKTRACE`), network (the proxy variables in both cases,
/// `SSL_CERT_FILE`, `SSL_CERT_DIR`) and process (`NODE_OPTIONS`, `DYLD_*`, `LD_*`); a server may
/// name those only at the value the shell or `plan.env` already gives them.
fn reserved_env(name: &str) -> Reserved {
    if name == "TERM"
        || ["GIT_CONFIG_", "OCTOBOARD_"]
            .iter()
            .any(|p| name.starts_with(p))
    {
        return Reserved::Refused;
    }
    let held_name = [
        "RUST_LOG",
        "RUST_BACKTRACE",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "NODE_OPTIONS",
    ]
    .contains(&name)
        || ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"]
            .iter()
            .any(|proxy| name.eq_ignore_ascii_case(proxy))
        || ["CODEX_", "OPENAI_", "DYLD_", "LD_"]
            .iter()
            .any(|prefix| name.starts_with(prefix));
    if held_name {
        Reserved::Held
    } else {
        Reserved::No
    }
}

/// The instructions that have Codex read Claude Code's configuration (`claude_compatibility.md`),
/// with `{CLAUDE_CONFIG_DIR}` filled in by [`claude_compatibility_instructions`].
const CLAUDE_COMPATIBILITY_INSTRUCTIONS: &str = include_str!("claude_compatibility.md");

/// The instructions that have Codex read Claude Code's instructions and skills, naming the
/// user-level configuration directory of Claude Code's default account as the launch's shell
/// snapshot resolves it (`~/.claude` when it names none).
fn claude_compatibility_instructions(shell_env: &HashMap<String, String>) -> String {
    CLAUDE_COMPATIBILITY_INSTRUCTIONS.replace(
        "{CLAUDE_CONFIG_DIR}",
        &super::default_account_dir(Agent::Claude, shell_env).to_string_lossy(),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;

    #[test]
    fn verified_private_server_keeps_client_and_server_overrides() {
        let mut fixture = spec_fixture();
        fixture.agent_version = Some((0, 163, 0));
        fixture.model = Some("probe-model".to_string());
        fixture.effort = Some("high".to_string());
        let mut spec = fixture.spec();
        spec.task = Some("first task");
        let plan = CodexAdapter.plan(&spec).unwrap();
        let private = plan.codex_server.as_ref().unwrap();
        assert_eq!(overrides(&private.args), overrides(&plan.args));
        assert_eq!(private.tui_args, plan.args);
        assert_eq!(&private.tui_args[..2], ["--cd", spec.cwd.to_str().unwrap()]);
        assert_eq!(
            private.tui_args.last().map(String::as_str),
            Some("first task")
        );
        assert_eq!(private.hook_command, spec.hook_script.to_string_lossy());
        spec.resume_agent_session_id = Some("thread-to-resume");
        let resumed = CodexAdapter.plan(&spec).unwrap();
        let private = resumed.codex_server.unwrap();
        assert_eq!(private.resume_thread.as_deref(), Some("thread-to-resume"));
        assert_eq!(private.tui_args, resumed.args);
        assert_eq!(&private.tui_args[..2], ["resume", "thread-to-resume"]);
        assert_eq!(
            &private.tui_args[2..4],
            ["--cd", spec.cwd.to_str().unwrap()]
        );
        fixture.agent_version = None;
        assert!(CodexAdapter
            .plan(&fixture.spec())
            .unwrap()
            .codex_server
            .is_none());
        fixture.agent_version = Some((0, 164, 0));
        assert!(CodexAdapter
            .plan(&fixture.spec())
            .unwrap()
            .codex_server
            .is_none());
    }

    #[test]
    fn standalone_installs_report_the_version_from_the_resolved_package() {
        let fixture = crate::test_support::ScratchDir::new("codex-installed-version");
        std::fs::create_dir_all(fixture.join("release/bin")).unwrap();
        std::fs::write(fixture.join("release/bin/codex"), "").unwrap();
        std::fs::write(
            fixture.join("release/codex-package.json"),
            r#"{"version":"0.163.0"}"#,
        )
        .unwrap();
        std::os::unix::fs::symlink(fixture.join("release/bin/codex"), fixture.join("codex"))
            .unwrap();
        assert_eq!(
            CodexAdapter.installed_version(fixture.join("codex").to_str().unwrap()),
            Some((0, 163, 0))
        );
    }

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

    #[test]
    fn a_project_name_codex_cut_short_is_still_the_project() {
        let names = ["f1120ca1-f304-4489-aa46-2ac04c2297d1".to_owned()];
        let task = |raw| CodexAdapter.task_title(raw, &names);
        assert_eq!(task("f1120ca1-f304-4489-aa..."), None);
        assert_eq!(task("⠙ | f1120ca1-f304-4489-aa..."), None);
        assert_eq!(
            task("Uninstall codexbar | f1120ca1-f304-4489-aa...").as_deref(),
            Some("Uninstall codexbar")
        );
        assert_eq!(
            task("Fix it | f1120ca1-xxxx...").as_deref(),
            Some("Fix it | f1120ca1-xxxx..."),
            "a cut name must begin the project's"
        );
        assert_eq!(task("Fix it | ...").as_deref(), Some("Fix it | ..."));
        let short = ["proj".to_owned()];
        assert_eq!(
            CodexAdapter.task_title("x | proj...", &short).as_deref(),
            Some("x | proj..."),
            "a name Codex keeps whole is never cut"
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
                Agent::Codex,
                &fixture.spec().role_options(),
            ))
        ))));
        assert!(!overrides.iter().any(|o| o.starts_with("instructions=")));
    }

    /// Claude compatibility appends the instructions that have Codex read Claude Code's
    /// configuration after the role text, naming the directory the launch's Claude Code default
    /// account resolves to; off, the instructions are the role text alone.
    #[test]
    fn claude_compatibility_appends_the_instructions_naming_the_claude_config_dir() {
        let instructions = |plan: &crate::adapter::LaunchPlan| {
            overrides(&plan.args)
                .into_iter()
                .find(|o| o.starts_with("developer_instructions="))
                .expect("developer_instructions")
        };
        let mut fixture = spec_fixture();
        fixture.shell_env.insert(
            "CLAUDE_CONFIG_DIR".to_string(),
            "/work/claude-home".to_string(),
        );
        let off = instructions(&CodexAdapter.plan(&fixture.spec()).unwrap());
        assert!(!off.contains("Claude Code compatibility"));

        let mut spec = fixture.spec();
        spec.claude_compatibility = true;
        let on = instructions(&CodexAdapter.plan(&spec).unwrap());
        let role = toml_string(&mcp::role::role_description(
            fixture.role,
            fixture.owner.as_ref(),
            Agent::Codex,
            &spec.role_options(),
        ));
        let role_text = role.trim_end_matches('"');
        assert!(on.starts_with(&format!("developer_instructions={role_text}")));
        assert!(on.contains("# Claude Code compatibility"));
        assert!(on.contains("/work/claude-home"));
        assert!(!on.contains("{CLAUDE_CONFIG_DIR}"));
    }

    /// With Claude compatibility on, Claude Code's servers reach the launch as overrides and the
    /// values that may be secret as environment, never argv; a server the account's own
    /// `config.toml` defines is left alone; off, nothing is added.
    #[test]
    fn claude_compatibility_passes_claude_codes_mcp_servers_without_secrets_on_argv() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("home");
        let codex_home = fixture.scratch.join("codex-home");
        std::fs::create_dir_all(&codex_home).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            codex_home.join("config.toml"),
            "[mcp_servers.mine]\ncommand = \"mine\"\n",
        )
        .unwrap();
        std::fs::write(
            home.join(".claude.json"),
            serde_json::json!({"mcpServers": {
                "remote": {"type": "http", "url": "https://x.example/mcp",
                           "headers": {"Authorization": "Bearer hunter2"}},
                "mine": {"command": "claude-mine"},
                "octoboard": {"command": "claude-octoboard"}
            }})
            .to_string(),
        )
        .unwrap();
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        fixture.config_dir = Some(codex_home);

        let plan = CodexAdapter.plan(&fixture.spec()).unwrap();
        assert!(!overrides(&plan.args)
            .iter()
            .any(|o| o.starts_with("mcp_servers.remote")));

        let mut spec = fixture.spec();
        spec.claude_compatibility = true;
        let plan = CodexAdapter.plan(&spec).unwrap();
        let all = overrides(&plan.args);
        assert!(all.contains(&"mcp_servers.remote.url=\"https://x.example/mcp\"".to_string()));
        assert!(all
            .iter()
            .any(|o| o.starts_with("mcp_servers.remote.env_http_headers=")));
        assert!(!all.iter().any(|o| o.contains("hunter2")));
        assert!(!all.iter().any(|o| o.ends_with("command=\"claude-mine\"")
            || o.ends_with("command=\"claude-octoboard\"")));
        assert!(plan.env.iter().any(|(name, value)| {
            name.starts_with("OCTOBOARD_CLAUDE_MCP_") && value == "Bearer hunter2"
        }));
    }

    /// A server whose `env` sets a variable the launch or Codex owns is skipped as a whole; an
    /// ordinary one beside them still passes.
    #[test]
    fn claude_compatibility_skips_servers_that_set_what_the_launch_or_codex_owns() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("home");
        let codex_home = fixture.scratch.join("codex-home");
        std::fs::create_dir_all(&codex_home).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let server =
            |key: &str, value: &str| serde_json::json!({"command": "c", "env": {key: value}});
        std::fs::write(
            home.join(".claude.json"),
            serde_json::json!({"mcpServers": {
                "home": server("CODEX_HOME", "/elsewhere"),
                "gitcfg": server("GIT_CONFIG_COUNT", "0"),
                "octo": server("OCTOBOARD_X", "1"),
                "log": server("RUST_LOG", "trace"),
                "proxy": server("https_proxy", "http://p"),
                "ordinary": server("MY_KEY", "v")
            }})
            .to_string(),
        )
        .unwrap();
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        // The shell already holds the value the server asks for, so only `Refused` skips it.
        fixture
            .shell_env
            .insert("GIT_CONFIG_COUNT".to_string(), "0".to_string());
        fixture.config_dir = Some(codex_home);

        let mut spec = fixture.spec();
        spec.claude_compatibility = true;
        let plan = CodexAdapter.plan(&spec).unwrap();
        let all = overrides(&plan.args);
        for skipped in ["home", "gitcfg", "octo", "log", "proxy"] {
            let prefix = format!("mcp_servers.{skipped}.");
            assert!(!all.iter().any(|o| o.starts_with(&prefix)), "{skipped}");
        }
        assert!(all.contains(&"mcp_servers.ordinary.command=\"c\"".to_string()));
        assert_eq!(
            plan.env
                .iter()
                .filter(|(name, _)| name == "CODEX_HOME")
                .count(),
            1
        );
        assert!(plan.env.contains(&("MY_KEY".to_string(), "v".to_string())));
    }

    /// A server the project's `.codex/config.toml` defines, in the working directory or up to the
    /// repository root, is left alone; one that cannot be read leaves nothing to compare with.
    #[test]
    fn claude_compatibility_leaves_servers_the_projects_codex_config_defines() {
        let mut fixture = spec_fixture();
        let repo = fixture.scratch.join("repo");
        let cwd = repo.join("sub");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(cwd.join(".codex")).unwrap();
        std::fs::create_dir_all(repo.join(".codex")).unwrap();
        std::fs::write(
            cwd.join(".codex/config.toml"),
            "[mcp_servers.near]\ncommand = \"n\"\n",
        )
        .unwrap();
        std::fs::write(
            repo.join(".codex/config.toml"),
            "[mcp_servers.root]\ncommand = \"r\"\n",
        )
        .unwrap();
        let home = fixture.scratch.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(".claude.json"),
            serde_json::json!({"mcpServers": {
                "near": {"command": "c"}, "root": {"command": "c"}, "free": {"command": "c"}
            }})
            .to_string(),
        )
        .unwrap();
        fixture
            .shell_env
            .insert("HOME".to_string(), home.to_string_lossy().into_owned());
        fixture.cwd = cwd.clone();
        let mut spec = fixture.spec();
        spec.claude_compatibility = true;
        let all = overrides(&CodexAdapter.plan(&spec).unwrap().args);
        assert!(all.contains(&"mcp_servers.free.command=\"c\"".to_string()));
        assert!(!all.iter().any(|o| o.starts_with("mcp_servers.near.")));
        assert!(!all.iter().any(|o| o.starts_with("mcp_servers.root.")));

        std::fs::write(cwd.join(".codex/config.toml"), "= not toml").unwrap();
        let mut spec = fixture.spec();
        spec.claude_compatibility = true;
        let all = overrides(&CodexAdapter.plan(&spec).unwrap().args);
        assert!(!all.iter().any(|o| o.starts_with("mcp_servers.free.")));
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

    #[test]
    fn remote_resumes_pass_the_reviewer_to_the_server_only() {
        use crate::protocol::ApprovalReviewer;
        let mut fixture = spec_fixture();
        fixture.agent_version = Some((0, 163, 0));
        for reviewer in [ApprovalReviewer::User, ApprovalReviewer::AutoReview] {
            for resume in [None, Some("conversation-1")] {
                let mut spec = fixture.spec();
                spec.approval_reviewer = Some(reviewer);
                spec.resume_agent_session_id = resume;
                let plan = CodexAdapter.plan(&spec).unwrap();
                let private = plan.codex_server.as_ref().unwrap();
                assert_eq!(private.resume_reviewer, Some(reviewer));
                let server_overrides = overrides(&private.args);
                let client_overrides = overrides(&private.tui_args);
                let reviewer_override = server_overrides
                    .iter()
                    .find(|value| value.starts_with("approvals_reviewer="))
                    .unwrap();
                if resume.is_some() {
                    assert_eq!(&private.tui_args[..2], ["resume", "conversation-1"]);
                    assert!(!client_overrides.contains(reviewer_override));
                    assert_eq!(
                        client_overrides,
                        server_overrides
                            .iter()
                            .filter(|value| *value != reviewer_override)
                            .cloned()
                            .collect::<Vec<_>>()
                    );
                } else {
                    assert_eq!(private.tui_args, plan.args);
                    assert!(client_overrides.contains(reviewer_override));
                }
            }
        }
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

    /// A FIFO at `config.toml` or at the models cache counts as unreadable instead of holding the
    /// launch until a writer appears, and so does either file above its size bound.
    #[test]
    fn a_fifo_or_an_oversized_config_counts_as_unreadable() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("codex-home");
        std::fs::create_dir_all(&home).expect("codex home");
        fixture.shell_env.insert(
            "CODEX_HOME".to_string(),
            home.to_string_lossy().into_owned(),
        );
        let config = home.join("config.toml");
        for path in [&config, &home.join("models_cache.json")] {
            let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        }
        let (done, finished) = std::sync::mpsc::channel();
        let fixture = std::thread::spawn(move || {
            let planned = CodexAdapter.plan(&fixture.spec()).is_ok();
            let _ = done.send(planned);
            fixture
        });
        assert_eq!(
            finished.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(true),
            "the launch was held by a FIFO or did not plan"
        );
        let fixture = fixture.join().expect("planned");

        let cache = home.join("models_cache.json");
        std::fs::remove_file(&cache).expect("FIFO removed");
        let models = json!({ "models": [{ "slug": "a", "upgrade": { "model": "b" } }] });
        std::fs::write(&cache, models.to_string()).expect("cache");
        assert!(model_migrations(&home).is_some());
        let mut padded = models;
        padded["pad"] = json!("x".repeat(JSON_FILE_LIMIT as usize));
        std::fs::write(&cache, padded.to_string()).expect("cache");
        assert_eq!(model_migrations(&home), None);

        std::fs::remove_file(&config).expect("FIFO removed");
        std::fs::write(
            &config,
            format!(
                "approvals_reviewer = \"auto_review\"\n# {}\n",
                "x".repeat(SMALL_FILE_LIMIT)
            ),
        )
        .expect("config");
        assert!(matches!(read_config(&home), ConfigFile::Unreadable));
        assert!(
            !CodexAdapter
                .plan(&fixture.spec())
                .expect("plan")
                .resolves_approvals_itself
        );
    }

    /// A home directory opened as the project holds the user's own Codex home, which a dotfiles
    /// manager may link elsewhere: its `config.toml` is read as Codex reads it, not refused as a
    /// link out of the project.
    #[test]
    fn a_linked_codex_home_under_a_home_opened_as_the_project_is_read() {
        let home = crate::test_support::ScratchDir::new("codex-home-as-project");
        let home = canonical_project(&home);
        let dotfiles = crate::test_support::ScratchDir::new("codex-dotfiles");
        std::fs::create_dir(dotfiles.join("codex")).expect("linked home");
        std::fs::write(
            dotfiles.join("codex/config.toml"),
            "[mcp_servers.mine]\ncommand = \"x\"\n",
        )
        .expect("config");
        std::os::unix::fs::symlink(dotfiles.join("codex"), home.join(".codex")).expect("link");
        assert_eq!(
            project_server_names(&home, &home.join(".codex")),
            Some(BTreeSet::from(["mine".to_string()]))
        );
    }

    /// The title items are pinned for every launch, so a project's own `terminal_title = []` cannot
    /// leave the trust watch without its start signal; a resume starts in the current directory,
    /// and the nudge to switch model is hidden.
    #[test]
    fn every_launch_pins_the_terminal_title_and_hides_the_model_nudge() {
        let fixture = spec_fixture();
        let fresh = overrides(&CodexAdapter.plan(&fixture.spec()).expect("plan").args);
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some("thread-id");
        let resumed = overrides(&CodexAdapter.plan(&spec).expect("plan").args);
        for overrides in [&fresh, &resumed] {
            assert!(overrides.contains(
                &"tui.terminal_title=[\"activity\",\"thread-name\",\"project-name\"]".to_string()
            ));
            assert!(overrides.contains(&"notice.hide_rate_limit_model_nudge=true".to_string()));
        }
        assert!(!fresh.iter().any(|o| o.starts_with("tui.resume_cwd")));
        assert!(resumed.contains(&"tui.resume_cwd=\"current\"".to_string()));
    }

    const MODELS_CACHE: &str = r#"{"models":[
        {"slug":"gpt-6-sol","upgrade":null},
        {"slug":"gpt-5.6-sol","upgrade":{
            "model":"gpt-6-sol","migration_markdown":"Meet GPT-6 Sol\n"}},
        {"slug":"gpt-5.5","upgrade":{"model":"gpt-6-sol","migration_markdown":"x"}},
        {"slug":"gpt-5.5","upgrade":{"model":"repeated"}},
        {"slug":"no-upgrade-key"},
        {"slug":"odd","upgrade":{"markdown":"no target"}},
        {"upgrade":{"model":"no-slug"}}]}"#;

    #[test]
    fn the_migration_table_maps_every_model_that_has_an_upgrade() {
        let table = upgrade_table(MODELS_CACHE).expect("table");
        assert_eq!(
            table,
            "{\"gpt-5.6-sol\"=\"gpt-6-sol\",\"gpt-5.5\"=\"gpt-6-sol\"}"
        );
        let parsed: toml::Table = format!("v={table}").parse().expect("valid TOML");
        assert_eq!(parsed["v"]["gpt-5.5"].as_str(), Some("gpt-6-sol"));
    }

    #[test]
    fn a_models_cache_that_says_nothing_gives_no_migration_table() {
        for cache in [
            "",
            "not json",
            "[]",
            "{}",
            r#"{"models":{}}"#,
            r#"{"models":[{"slug":"a","upgrade":null}]}"#,
        ] {
            assert_eq!(upgrade_table(cache), None, "{cache:?}");
        }
    }

    /// The models cache is read from the Codex home in effect, the pinned directory when there is
    /// one, and nothing is passed when there is none.
    #[test]
    fn the_migration_override_comes_from_the_codex_home_in_effect() {
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
        let migrations = |fixture: &crate::adapter::tests::SpecFixture| {
            overrides(&CodexAdapter.plan(&fixture.spec()).expect("plan").args)
                .into_iter()
                .find(|o| o.starts_with("notice.model_migrations="))
        };
        assert_eq!(migrations(&fixture), None, "no cache");

        std::fs::write(shell_home.join("models_cache.json"), MODELS_CACHE).expect("cache");
        assert_eq!(
            migrations(&fixture).as_deref(),
            Some(
                "notice.model_migrations={\"gpt-5.6-sol\"=\"gpt-6-sol\",\"gpt-5.5\"=\"gpt-6-sol\"}"
            )
        );

        fixture.config_dir = Some(pinned.clone());
        assert_eq!(migrations(&fixture), None, "the pinned home has its own");
        std::fs::write(
            pinned.join("models_cache.json"),
            r#"{"models":[{"slug":"a","upgrade":{"model":"b"}}]}"#,
        )
        .expect("cache");
        assert_eq!(
            migrations(&fixture).as_deref(),
            Some("notice.model_migrations={\"a\"=\"b\"}")
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
