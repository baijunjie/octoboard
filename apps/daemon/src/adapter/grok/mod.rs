//! Grok Build adapter, the most intricate of the three: Grok has no flag for hooks at all.
//!
//! What does not work, and fails silently: `GROK_CONFIG` / `GROK_CONFIG_PATH` layer onto
//! `config.toml` for an allowlist of keys that drops `hooks` and `mcp_servers`;
//! `$GROK_HOME/managed_config.toml` and `requirements.toml` load but are deleted by the deployment
//! sync at runtime; `grok mcp add` only writes persistently to user or project scope.
//!
//! What works is a per-session `GROK_HOME` built as a symlink farm over the source home: the
//! console's pinned directory when set, else the shell's `GROK_HOME`, else `~/.grok`. Every entry is
//! a symlink to the source's except the ones Octoboard has to own. A link leads straight to the
//! entry's fully resolved target, so a source entry that is itself a link (an account sharing the
//! default configuration) is reached through one link, the same way that account's own link leads
//! straight at the default entry's target, and a save does not depend on following more than one
//! link. A dangling source link keeps pointing at the source entry. `auth.json` and `sessions/`
//! stay symlinks, so login state is shared and a session Octoboard started stays resumable from the
//! user's own `grok` — plain for the default home, with `GROK_HOME=<that directory>` for a pinned
//! one.
//!
//! Two files in the farm are copies rather than symlinks, and both must stay that way:
//!
//! - `config.toml`. The session home keeps a real copy so the MCP server appended to it stays out of
//!   the source file. Grok rewrites the copy for session settings. When the source file is a symlink,
//!   the theme, the plugin switches, and a model chosen in the session are written to that link's
//!   canonical target after the process exits ([`writeback::SharedConfigWriteback`]), and the link
//!   itself is left in place. The model and reasoning effort passed only for this launch are not
//!   written there, and neither is the MCP block.
//! - `trusted_folders.toml`: Grok saves it by replacing the file, so against a symlink its write
//!   would replace the link rather than reach the user's store, and no setting moves the store out
//!   of `GROK_HOME`. The copy carries every folder the user already trusts, and Octoboard adds
//!   nothing to it: a folder Grok has not been told to trust shows Grok's own confirmation, which
//!   Octoboard presses under the user's permission (`crate::session::trust`), or the person answers
//!   in the terminal. The entry Grok then writes lands only in this copy, so once the confirmation
//!   is accepted, by Octoboard's press or in the terminal, it is carried over, unchanged, to the
//!   user's own store (`LaunchPlan::carried_trust`, `trust`), and a later launch is trusted from
//!   the start. Until a folder is trusted Grok silently loads none of the project's own
//!   `AGENTS.md`, hooks or MCP servers. Folder trust, not a git repository, is what gates the
//!   instruction files: outside one Grok reads only the working directory's own.
//!
//! Four more entries are real copies too, `bundled/`, `bundles/`, `hooks-paths` and
//! `sandbox.toml`: Grok's startup sandbox check refuses to start when they are symlinks (it
//! documents that it write-protects them under its sandbox). They are copied on every launch, even
//! though a sandboxed launch is refused (see `SANDBOX_CHECKED_ENTRIES` for why). A source that is
//! itself a symlink becomes a real file or directory in the farm, which is what Grok checks. An entry the source lacks stays absent, for Grok to create. A bundle
//! update Grok downloads during a session lands in the copy and is lost; the next launch copies
//! whatever the user's own `grok` has fetched since.
//!
//! A launch that would run under Grok's sandbox is refused up front (`require_unsandboxed`),
//! because the sandbox lets Grok write only to its home, the working directory and temp
//! directories, with links resolved: the farm's `sessions/` and `auth.json` links lead back into
//! the source home, so creating a session would fail. The profile is read from the sources Grok
//! layers (see `sandbox_setting`); macOS managed preferences (`ai.x.grok`) are not read, a known
//! gap that leaves such a launch to fail inside Grok. The home's own `requirements.toml` and
//! `managed_config.toml` are not read either, because the farm excludes them and Grok never loads
//! them from it; the system directory's are.
//!
//! A session pins its source home at creation, because the session records a resume looks for
//! live in that home's `sessions/`; the farm is still what Grok runs against, so nothing changes
//! except which directory the links and copies come from.
//!
//! A pinned directory must already be a real Grok home, one Grok has been run against: the farm only
//! links entries that exist in the source, so for an empty directory Grok would create `sessions/`
//! and `auth.json` inside the throw-away farm and lose them.
//!
//! The copies are a snapshot: an edit the user makes while a session runs is not seen, and anything
//! the session persists lands in Octoboard's copy. Two things are kept: the trust entry above, once
//! the confirmation is accepted, and, when the source `config.toml` is a symlink, the settings
//! [`writeback::SharedConfigWriteback`] copies to its target. The rest is lost with the process.
//! Rebuilding the farm on every launch is what picks up an edit made in between.
//!
//! Two conditions come from Grok itself rather than from the mechanism: it locates a project by
//! walking up for a `.git` directory and reads no project hooks without one; and its hooks must be
//! `command` type, because HTTP hooks cannot reach the daemon at all — Grok's SSRF protection
//! rejects both plain HTTP and private addresses. That same SSRF protection is why the injected
//! MCP server is a child process here too.
//!
//! A console's working directory is not a repository, but Grok still reads the instruction file
//! written there once the folder is trusted, which Octoboard's confirmation presses for a console
//! session. So a Grok console session gets the same short pointer in `--rules` as the other
//! agents, and the role itself from that file. Grok freezes both `--rules` and the instruction
//! files for a session, replaying them on a resume or a fork, so a change never reaches a session
//! already begun.

pub(crate) mod accounts;
mod conversation;
mod home;
mod hooks;
mod login;
pub(crate) mod models;
pub(crate) mod quota;
pub(crate) mod records;
mod sandbox;
pub(crate) mod screen;
mod shared_config;
mod switch_continue;
pub(crate) mod trust;
mod writeback;

#[cfg(test)]
mod home_tests;
#[cfg(test)]
mod sandbox_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod writeback_tests;

use std::path::Path;

use anyhow::Result;

use super::{AgentProvider, LaunchPlan, LaunchSpec};
use crate::protocol::{AgentId, Code, CodedError, TerminalAppearance};
use crate::providers::{grok, ProviderDescriptor};
use crate::session::screen::ScreenReader;
use crate::session_tools;
use home::{build_grok_home, canonical_cwd};
use sandbox::require_unsandboxed;
use writeback::shared_config_writeback;

/// The events Octoboard's session states are derived from. All three stop events are registered
/// because they are mutually exclusive, and `Notification` is the backstop: some turns (bash mode,
/// builtin slash commands, cancel-and-send, rewinds) report no stop event at all, and
/// `idle_prompt` is then the session's only turn-end signal.
const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionDenied",
    "Stop",
    "StopFailure",
    "StopCancelled",
    "Notification",
];

/// The status Grok shows after the spinner while it waits for the model, the only one seen with
/// no title after it.
const WAITING_STATUS: &str = "Waiting for response…";

/// Where Grok reads the fleet-wide `managed_config.toml` and `requirements.toml` from. The ones in
/// its home are excluded from the farm, so Grok running against it never loads them.
const SYSTEM_GROK_DIR: &str = "/etc/grok";

pub struct GrokProvider;

impl AgentProvider for GrokProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::GROK.descriptor()
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        &screen::GrokScreen
    }

    fn hooks(&self) -> &'static dyn crate::hooks::HookInterpreter {
        &hooks::GrokHooks
    }

    fn accounts(&self) -> Option<&'static dyn crate::accounts::AccountOps> {
        Some(&accounts::GrokAccounts)
    }

    fn catalogue(&self) -> &'static dyn crate::agents::models::ModelCatalogue {
        &models::GrokCatalogue
    }

    fn records(&self) -> &'static dyn crate::records::RecordStore {
        &records::GrokRecords
    }

    fn refuse_unlaunchable_account(&self, pinned: Option<&Path>) -> Result<()> {
        match pinned {
            Some(dir) => require_initialized_grok_home(dir),
            None => Ok(()),
        }
    }

    /// Native releases resolve to a versioned download, without spawning a CLI process.
    fn installed_version(&self, binary: &str) -> Option<super::AgentVersion> {
        let resolved = std::fs::canonicalize(binary).ok()?;
        let filename = resolved.file_name()?.to_str()?;
        let release = filename.strip_prefix("grok-")?;
        let (version, platform) = release.split_once('-')?;
        if !matches!(
            platform,
            "macos-aarch64" | "macos-x86_64" | "linux-aarch64" | "linux-x86_64"
        ) {
            return None;
        }
        super::parse_version(version)
    }

    /// Grok's title is its items joined by ` - `: while it works a spinner and a status ending in
    /// an ellipsis, then the conversation's title, then its own name last. The item after the
    /// spinner is a status when a title still follows it, or when it is the known wait status.
    fn task_title(&self, raw: &str, _project_names: &[String]) -> Option<String> {
        // The terminal title reaches here without the spinner it starts with, which leaves the
        // separator after it at the front.
        let (raw, mut spun) = match raw.strip_prefix("- ") {
            Some(rest) => (rest, true),
            None => (raw, false),
        };
        let mut items: Vec<&str> = raw.split(" - ").map(str::trim).collect();
        if items.first().is_some_and(|first| super::is_spinner(first)) {
            items.remove(0);
            spun = true;
        }
        if spun {
            let titled = items.iter().filter(|item| **item != "grok").count() > 1;
            let status = |item: &&str| item.ends_with('…') && (titled || *item == WAITING_STATUS);
            if items.first().is_some_and(status) {
                items.remove(0);
            }
        }
        if items.last() == Some(&"grok") {
            items.pop();
        }
        items.retain(|item| !item.is_empty());
        (!items.is_empty()).then(|| items.join(" - "))
    }

    /// `generated_title` of the session directory's `summary.json`.
    fn summary_title(&self, summary: &serde_json::Value) -> Option<String> {
        Some(summary.get("generated_title")?.as_str()?.to_owned())
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        plan_under(spec, Path::new(SYSTEM_GROK_DIR))
    }
}

/// `plan` with the directory of Grok's system-wide configuration passed in, so a test never
/// reads the machine's own.
fn plan_under(spec: &LaunchSpec<'_>, system_dir: &Path) -> Result<LaunchPlan> {
    let pinned = super::pinned_config_dir(spec, AgentId::GROK)?;
    // Unlike Claude Code and Codex, an account Grok has never been run against cannot simply be
    // created on first use: the per-session home only links what already exists in the source,
    // so an empty one would have Grok write its login and conversation into the throw-away
    // farm instead, discarded with the process — the exact silent failure a switch must not
    // produce. Checked only for a *pinned* source home: the default's own (the shell's
    // `GROK_HOME`, or `~/.grok`) is the user's existing setup, which is not Octoboard's to
    // second-guess.
    if let Some(dir) = pinned {
        require_initialized_grok_home(dir)?;
    }
    let source_home = super::account_dir(AgentId::GROK, pinned, spec.shell_env);
    require_unsandboxed(spec, &source_home, system_dir)?;
    let grok_home = build_grok_home(spec, &source_home)?;

    // Grok's leader mode (`[cli] use_leader = true` in the user's configuration, which the farm
    // copies) makes this TUI only a client of a leader process it starts in a process group of
    // its own but in the TUI's session. The TUI exits at once on the stop's `SIGTERM` without
    // ending the turn, so the leader is reached only by the sweep of the exited TUI's session, a
    // `SIGKILL` mid-turn that fires no `shutdown` hooks and skips the session flush of Grok's own
    // shutdown.
    //
    // `--no-leader`, a TUI flag `grok --help` does not list, keeps the agent in this process,
    // which the `SIGTERM` ends gracefully, whatever the configuration says.
    let mut args = vec!["--no-leader".to_string()];
    match spec.resume_agent_session_id {
        // The id is required: a bare `--resume` takes the next argument that is not a known
        // flag, such as the task, as its value.
        Some(id) => {
            args.push("--resume".to_string());
            args.push(id.to_string());
        }
        None => {
            args.push("--session-id".to_string());
            args.push(spec.new_agent_session_id.to_string());
        }
    }

    // `--rules` appends to the system prompt and persists into the session record. It is
    // passed on every launch anyway, so a session whose record predates the current text is
    // not left without a role.
    args.push("--rules".to_string());
    args.push(session_tools::role::role_description(
        spec.kind,
        spec.owner.as_ref(),
        AgentId::GROK,
        &spec.role_options(),
    ));

    // A fullscreen launch writes `--model` into `[models] default` and `--reasoning-effort` into
    // `[models] default_reasoning_effort` before the user changes anything. The writeback skips a
    // default equal to this launch's model and never copies the effort, so neither flag replaces
    // the stored value. A model the user selects in the session, different from this one, is copied.
    if let Some(model) = spec.model {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    if let Some(effort) = spec.effort {
        args.push("--reasoning-effort".to_string());
        args.push(effort.to_string());
    }

    if spec.resume_agent_session_id.is_none() {
        if let Some(task) = spec.task {
            args.push(task.to_string());
        }
    }

    let carried_trust = trust::CarriedTrust {
        session_store: grok_home.join(grok::GROK_TRUST_STORE),
        user_store: source_home.join(grok::GROK_TRUST_STORE),
        folder: canonical_cwd(spec.cwd),
        record_wait: trust::RECORD_TIMEOUT,
    };
    // Grok has no `--theme` flag. `GROK_THEME` outranks `config.toml`, so the copy of the user's
    // configuration cannot paint a dark theme onto the light terminal (or the reverse). A launch
    // that names no appearance uses `auto`, which follows the operating system, the same source as
    // the window's System choice.
    let theme = match spec.terminal_appearance {
        Some(TerminalAppearance::Light) => "grokday",
        Some(TerminalAppearance::Dark) => "groknight",
        None => "auto",
    };
    let mut env = vec![
        (
            grok::LAYOUT.config_dir_var.to_string(),
            grok_home.to_string_lossy().into_owned(),
        ),
        ("GROK_THEME".to_string(), theme.to_string()),
    ];
    // Grok's own Claude compatibility is on unless its configuration or the user's shell turns it
    // off; these outrank both. Turned off in Octoboard, nothing is passed and Grok follows its own
    // configuration.
    if spec.settings.switch("claude_compatibility") {
        env.extend(CLAUDE_COMPATIBILITY_VARS.map(|var| (var.to_string(), "true".to_string())));
    }
    Ok(LaunchPlan {
        args,
        env,
        carried_trust: Some(std::sync::Arc::new(carried_trust)),
        after_exit: shared_config_writeback(&source_home, &grok_home, spec.model)
            .map(|writeback| Box::new(writeback) as Box<dyn crate::session::term::AfterExit>),
        ..LaunchPlan::default()
    })
}

/// The environment variables of Grok's `compat.claude.{skills,rules,agents,mcps,hooks}`
/// configuration, which outrank the configuration files.
const CLAUDE_COMPATIBILITY_VARS: [&str; 5] = [
    "GROK_CLAUDE_SKILLS_ENABLED",
    "GROK_CLAUDE_RULES_ENABLED",
    "GROK_CLAUDE_AGENTS_ENABLED",
    "GROK_CLAUDE_MCPS_ENABLED",
    "GROK_CLAUDE_HOOKS_ENABLED",
];

/// Refuses a pinned source home Grok has never been run against: it carries no `sessions/`
/// directory for the per-session farm to link, so a conversation Grok writes during the session
/// goes into the farm instead and is discarded with the process — the exact silent failure this
/// check exists to prevent. Whether the home also holds a login (`auth.json`) says nothing either
/// way: an API-key user's legitimate home has `sessions/` with no `auth.json`, and that is not
/// Octoboard's business to check.
pub(super) fn require_initialized_grok_home(dir: &Path) -> Result<()> {
    if dir.join(grok::GROK_RECORDS_DIR).is_dir() {
        return Ok(());
    }
    Err(CodedError::raised(
        Code::of_provider(grok::HOME_NOT_INITIALIZED),
        format!(
            "`{}` is not a Grok home yet: run `grok` against it directly at least once so it has \
             a session history, before pointing an account at it",
            dir.display()
        ),
        &[("path", &dir.to_string_lossy())],
    ))
}
