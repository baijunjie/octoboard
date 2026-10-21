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
//!   canonical target after the process exits ([`SharedConfigWriteback`]), and the link itself is left
//!   in place. The model and reasoning effort passed only for this launch are not written there, and
//!   neither is the MCP block.
//! - `trusted_folders.toml`: Grok saves it by replacing the file, so against a symlink its write
//!   would replace the link rather than reach the user's store, and no setting moves the store out
//!   of `GROK_HOME`. The copy carries every folder the user already trusts,
//!   and Octoboard adds nothing to it: a folder Grok has not been told to trust shows Grok's own
//!   confirmation, which Octoboard presses under the user's permission (`crate::trust`), or the
//!   person answers in the terminal. The entry Grok then writes lands only in this copy, so once
//!   the confirmation is accepted, by Octoboard's press or in the terminal, it is carried over,
//!   unchanged, to the user's own store (`LaunchPlan::carried_trust`), and a later launch is
//!   trusted from the start. Until a folder is trusted Grok silently loads none of the project's
//!   own `AGENTS.md`, hooks or MCP servers. Folder trust, not a git repository, is what gates the
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
//! [`SharedConfigWriteback`] copies to its target. The rest is lost with the process. Rebuilding the
//! farm on every launch is what picks up an edit made in between.
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

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};
use crate::mcp;
use crate::protocol::{error_code, Agent, CodedError, TerminalAppearance};
use crate::trust::CarriedTrust;

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

/// Grok's trust store, inside `GROK_HOME`.
const TRUST_STORE: &str = "trusted_folders.toml";

/// Entries Octoboard owns in its `GROK_HOME` rather than linking to the user's.
const COPIED_ENTRIES: &[&str] = &["config.toml", TRUST_STORE];

/// Entries Grok's startup sandbox check wants as real files rather than symlinks; copied from the
/// source when it has them. They are copied on every launch, although a launch that runs under the
/// sandbox is refused: detection cannot see every source (macOS managed preferences, policy synced
/// from the server), and a launch it misses must still get past Grok's startup check rather than
/// fail there. Copying unconditionally also avoids depending on detection.
const SANDBOX_CHECKED_ENTRIES: &[&str] = &["bundled", "bundles", "hooks-paths", "sandbox.toml"];

/// Entries that must never exist in the farm: Grok's deployment sync deletes them at runtime, and
/// an organisation-distributed config is not Octoboard's to reproduce.
const EXCLUDED_ENTRIES: &[&str] = &["managed_config.toml", "requirements.toml", "hooks"];

/// Where Grok reads the fleet-wide `managed_config.toml` and `requirements.toml` from. The ones in
/// its home are excluded from the farm, so Grok running against it never loads them.
const SYSTEM_GROK_DIR: &str = "/etc/grok";

pub struct GrokAdapter;

impl AgentAdapter for GrokAdapter {
    fn binary(&self) -> &'static str {
        "grok"
    }

    fn preallocates_session_id(&self) -> bool {
        true
    }

    fn uses_hook_script(&self) -> bool {
        true
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
    let pinned = super::pinned_config_dir(spec, Agent::Grok)?;
    // Unlike the other two agents, an account Grok has never been run against cannot simply be
    // created on first use: the per-session home only links what already exists in the source,
    // so an empty one would have Grok write its login and conversation into the throw-away
    // farm instead, discarded with the process — the exact silent failure a switch must not
    // produce. Checked only for a *pinned* source home: the default's own (the shell's
    // `GROK_HOME`, or `~/.grok`) is the user's existing setup, which is not Octoboard's to
    // second-guess.
    if let Some(dir) = pinned {
        require_initialized_grok_home(dir)?;
    }
    let source_home = pinned
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_grok_home(spec));
    require_unsandboxed(spec, &source_home, system_dir)?;
    let grok_home = build_grok_home(spec, &source_home)?;

    let mut args = Vec::new();
    match spec.resume_agent_session_id {
        // The id is required: a bare `--resume` swallows whatever argument follows as its
        // value.
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
    args.push(mcp::role::role_description(
        spec.role,
        spec.owner.as_ref(),
        Agent::Grok,
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

    let carried_trust = CarriedTrust {
        session_store: grok_home.join(TRUST_STORE),
        user_store: source_home.join(TRUST_STORE),
        folder: canonical_cwd(spec.cwd),
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
    Ok(LaunchPlan {
        args,
        env: vec![
            (
                "GROK_HOME".to_string(),
                grok_home.to_string_lossy().into_owned(),
            ),
            ("GROK_THEME".to_string(), theme.to_string()),
        ],
        carried_trust: Some(carried_trust),
        shared_config_writeback: shared_config_writeback(&source_home, &grok_home, spec.model),
        ..LaunchPlan::default()
    })
}

/// Refuses a pinned source home Grok has never been run against: it carries no `sessions/`
/// directory for the per-session farm to link, so a conversation Grok writes during the session
/// goes into the farm instead and is discarded with the process — the exact silent failure this
/// check exists to prevent. Whether the home also holds a login (`auth.json`) says nothing either
/// way: an API-key user's legitimate home has `sessions/` with no `auth.json`, and that is not
/// Octoboard's business to check.
pub(super) fn require_initialized_grok_home(dir: &Path) -> Result<()> {
    if dir.join("sessions").is_dir() {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::GROK_HOME_NOT_INITIALIZED,
        format!(
            "`{}` is not a Grok home yet: run `grok` against it directly at least once so it has \
             a session history, before pointing an account at it",
            dir.display()
        ),
        &[("path", &dir.to_string_lossy())],
    ))
}

/// One sandbox profile in effect, and where it is set.
struct SandboxSetting {
    /// The environment variable, or the file, that sets it.
    source: String,
    profile: String,
}

/// Refuses a launch that would run under Grok's sandbox; see the module comment for why the farm
/// cannot support one.
fn require_unsandboxed(spec: &LaunchSpec<'_>, source_home: &Path, system_dir: &Path) -> Result<()> {
    let Some(setting) = sandbox_setting(spec, source_home, system_dir) else {
        return Ok(());
    };
    Err(CodedError::raised(
        error_code::GROK_SANDBOX_UNSUPPORTED,
        format!(
            "Octoboard's per-session Grok home is incompatible with Grok's sandbox (profile `{}`, \
             set by {}): to launch this session through Octoboard, turn the sandbox off where it \
             is set, or ask whoever manages that setting to. A conversation started under the \
             sandbox keeps it, so continue that one in Grok itself",
            setting.profile, setting.source
        ),
        &[("source", &setting.source), ("profile", &setting.profile)],
    ))
}

/// The sandbox profile Grok would apply to this launch, `None` when it would run unsandboxed or
/// the profile cannot be read reliably. A non-`off` `requirements.toml` pin (the system
/// directory's; the home's is excluded from the farm) refuses, and an `off` pin stops the search.
/// On a resume, either the profile saved with the session or `GROK_SANDBOX` in the launch
/// environment being non-`off` refuses, because Grok restores a saved profile and refuses a resume
/// whose requested one differs. Otherwise, and for a resume with no saved profile found,
/// `GROK_SANDBOX` beats the user's `config.toml`, which beats the system `managed_config.toml`.
/// `GROK_SANDBOX` is read from the launch environment, not the daemon's own.
fn sandbox_setting(
    spec: &LaunchSpec<'_>,
    source_home: &Path,
    system_dir: &Path,
) -> Option<SandboxSetting> {
    let on = |source: String, profile: String| {
        (profile != "off").then_some(SandboxSetting { source, profile })
    };
    let from_file = |dir: &Path, name: &str| {
        let file = dir.join(name);
        let profile = toml_sandbox_profile(&file)?;
        Some((file.to_string_lossy().into_owned(), profile))
    };
    let from_env = || {
        spec.shell_env
            .get("GROK_SANDBOX")
            .map(|profile| profile.trim())
            .filter(|profile| !profile.is_empty())
            .map(|profile| ("GROK_SANDBOX".to_string(), profile.to_string()))
    };

    if let Some((source, profile)) = from_file(system_dir, "requirements.toml") {
        return on(source, profile);
    }

    let saved = spec
        .resume_agent_session_id
        .and_then(|id| saved_sandbox_profile(source_home, id));
    if let Some(saved) = saved {
        return [Some(saved), from_env()]
            .into_iter()
            .flatten()
            .find_map(|(source, profile)| on(source, profile));
    }

    if let Some((source, profile)) = from_env() {
        return on(source, profile);
    }
    [
        (source_home, "config.toml"),
        (system_dir, "managed_config.toml"),
    ]
    .into_iter()
    .find_map(|(dir, name)| from_file(dir, name))
    .and_then(|(source, profile)| on(source, profile))
}

/// `[sandbox] profile` of a Grok TOML file; `None` when the file is absent, unreadable, or does
/// not set a non-empty one.
fn toml_sandbox_profile(file: &Path) -> Option<String> {
    let table = std::fs::read_to_string(file)
        .ok()?
        .parse::<toml::Table>()
        .ok()?;
    let profile = table.get("sandbox")?.get("profile")?.as_str()?.trim();
    (!profile.is_empty()).then(|| profile.to_string())
}

/// The profile Grok saved in the `summary.json` of conversation `id`, found one directory below
/// `sessions/` rather than by reproducing how Grok encodes the working directory into its name.
fn saved_sandbox_profile(source_home: &Path, id: &str) -> Option<(String, String)> {
    // The id joins a path below, so only the shape of an id Grok mints is looked up.
    if !crate::agent_records::plausible_id(id) {
        return None;
    }
    for entry in std::fs::read_dir(source_home.join("sessions"))
        .ok()?
        .flatten()
    {
        let summary = entry.path().join(id).join("summary.json");
        if !std::fs::symlink_metadata(&summary).is_ok_and(|meta| meta.is_file()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&summary) else {
            continue;
        };
        let document: serde_json::Value = serde_json::from_str(&text).ok()?;
        let profile = document.get("sandbox_profile")?.as_str()?.trim();
        return (!profile.is_empty())
            .then(|| (summary.to_string_lossy().into_owned(), profile.to_string()));
    }
    None
}

/// Builds the session's `GROK_HOME` and returns its path.
fn build_grok_home(spec: &LaunchSpec<'_>, source_home: &Path) -> Result<PathBuf> {
    let farm = spec.scratch.join("grok-home");
    if farm.exists() {
        std::fs::remove_dir_all(&farm).ok();
    }
    std::fs::create_dir_all(&farm).with_context(|| format!("creating {}", farm.display()))?;

    if source_home.is_dir() {
        for entry in std::fs::read_dir(source_home)
            .with_context(|| format!("reading {}", source_home.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if EXCLUDED_ENTRIES.contains(&name_str.as_ref())
                || COPIED_ENTRIES.contains(&name_str.as_ref())
                || SANDBOX_CHECKED_ENTRIES.contains(&name_str.as_ref())
            {
                continue;
            }
            let link = farm.join(&name);
            // The fully resolved target, so a source entry that is itself a link is reached through
            // one link and a save does not depend on following a chain.
            let target = std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            std::os::unix::fs::symlink(&target, &link)
                .with_context(|| format!("linking {}", link.display()))?;
        }
    }

    let config = farm.join("config.toml");
    copy_if_present(&source_home.join("config.toml"), &config)?;
    append_mcp_server(&config, spec)?;

    copy_if_present(&source_home.join(TRUST_STORE), &farm.join(TRUST_STORE))?;
    for name in SANDBOX_CHECKED_ENTRIES {
        copy_if_present(&source_home.join(name), &farm.join(name))?;
    }
    write_hooks(&farm, spec.hook_script)?;

    Ok(farm)
}

/// The user's own Grok home when the session pins none, read from the launch environment rather
/// than the daemon's: `GROK_HOME` is a user setting, and the daemon's own copy of it may belong to
/// another Octoboard session's farm when the daemon was started from inside one. Shared with
/// `crate::availability`, which shows the same directory as the default account's before any
/// session has opened.
fn default_grok_home(spec: &LaunchSpec<'_>) -> PathBuf {
    super::default_account_dir(Agent::Grok, spec.shell_env)
}

/// Appends Octoboard's MCP server to the farm's own `config.toml`. Appended rather than written
/// fresh, because this copy carries the user's whole Grok configuration and replacing it would be
/// the "do not override the user's configuration" failure in another form.
fn append_mcp_server(config: &Path, spec: &LaunchSpec<'_>) -> Result<()> {
    let (command, args) = super::mcp_server_command(spec);
    let mut content = std::fs::read_to_string(config).unwrap_or_default();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    // Grok abandons a tool call at a limit of its own, and tells the server with a
    // `notifications/cancelled` — which withdraws a waiting session request, ending
    // the wait before the user can answer and leaving a later approval nothing to act on. That
    // makes giving up destructive here, unlike Codex, which loses only the call's result, and
    // Grok's own default is documented far above a call Octoboard holds for the user
    // (`session_request::ANSWER_TIME_LIMIT`), but its exact value was never measured, so where it
    // truly falls is unknown (the "Grok Build" section of `docs/agent-cli-reference.md` has what is
    // and is not established). The limit is therefore pinned to a known value rather than left to
    // that default. Hence `super::MCP_TOOL_TIMEOUT_SECS`, where the reasoning for the value is.
    content.push_str(&format!(
        "\n[mcp_servers.{key}]\ncommand = {command}\nargs = [{args}]\n\
         tool_timeout_sec = {tool_timeout}\n",
        key = mcp::SERVER_KEY,
        tool_timeout = super::MCP_TOOL_TIMEOUT_SECS,
        command = json!(command),
        args = args
            .iter()
            .map(|arg| json!(arg).to_string())
            .collect::<Vec<_>>()
            .join(", "),
    ));
    std::fs::write(config, content).with_context(|| format!("writing {}", config.display()))?;
    Ok(())
}

/// Copies a file, or a directory with everything under it; an absent source is left absent. A
/// link is followed only at the top, so a linked source becomes a real file or directory in the
/// copy; inside a directory a link is recreated as a link rather than followed.
fn copy_if_present(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        copy_tree(from, to)
    } else if from.is_file() {
        std::fs::copy(from, to)
            .map(drop)
            .with_context(|| format!("copying {}", from.display()))
    } else {
        Ok(())
    }
}

/// The inside of `copy_if_present` for a directory: files are copied, links recreated as they are,
/// anything else (a FIFO, a socket) skipped, since opening one could block or fail the launch.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        let kind = entry
            .file_type()
            .with_context(|| format!("reading {}", source.display()))?;
        if kind.is_symlink() {
            let destination = std::fs::read_link(&source)
                .with_context(|| format!("reading link {}", source.display()))?;
            std::os::unix::fs::symlink(destination, &target)
                .with_context(|| format!("linking {}", target.display()))?;
        } else if kind.is_dir() {
            copy_tree(&source, &target)?;
        } else if kind.is_file() {
            std::fs::copy(&source, &target)
                .with_context(|| format!("copying {}", source.display()))?;
        }
    }
    Ok(())
}

/// The folder Grok records its trust for: the working directory as Grok resolves it, canonical
/// (`/private/tmp/…`, not `/tmp/…`).
fn canonical_cwd(cwd: &Path) -> String {
    std::fs::canonicalize(cwd)
        .unwrap_or_else(|_| cwd.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Writes Octoboard's hooks into the farm. Hooks placed in `$GROK_HOME/hooks/*.json` load at
/// `global` scope, which is always trusted — the folder-trust requirement applies to the project's
/// own `<project>/.grok/hooks`, not to these.
fn write_hooks(farm: &Path, hook_script: &Path) -> Result<()> {
    let hooks_dir = farm.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;
    let command = hook_script.to_string_lossy().into_owned();
    let mut hooks = serde_json::Map::new();
    for event in HOOK_EVENTS {
        hooks.insert(
            (*event).to_string(),
            json!([{
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "timeout": HOOK_TIMEOUT_SECS,
                }]
            }]),
        );
    }
    let document = json!({ "hooks": hooks });
    std::fs::write(
        hooks_dir.join("octoboard.json"),
        serde_json::to_vec_pretty(&document)?,
    )?;
    Ok(())
}

/// Keys a sharing account's session may copy from its `config.toml` onto the shared file.
///
/// The session file stays a real copy, so the MCP block appended to it is never this write. The
/// account entry has to still be a symlink when the process exits; the bytes go to that link's
/// canonical target, and the link is not replaced. A session that did not change a key does not
/// write it, so it does not put back a value another session has already published.
///
/// A fullscreen launch writes `--model` into `[models] default` and `--reasoning-effort` into
/// `[models] default_reasoning_effort` with no further action. The effort is never copied. The
/// default is copied only when it differs from the model this launch passed, which is what a model
/// chosen in the session looks like; choosing the launch's own model again cannot be told from the
/// launch write, and is left in the session copy.
#[derive(Debug)]
pub struct SharedConfigWriteback {
    session_config: PathBuf,
    account_config: PathBuf,
    baseline: ConfigSlice,
    launch_model: Option<String>,
}

/// The keys [`SharedConfigWriteback`] compares. `None` means the key is absent. An empty plugin
/// list is present and is copied: clearing the list is how a plugin is switched off.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigSlice {
    theme: Option<String>,
    enabled: Option<Vec<String>>,
    disabled: Option<Vec<String>>,
    model: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct ConfigChanges {
    theme: Option<String>,
    enabled: Option<Vec<String>>,
    disabled: Option<Vec<String>>,
    model: Option<String>,
}

impl ConfigChanges {
    fn is_empty(&self) -> bool {
        self.theme.is_none()
            && self.enabled.is_none()
            && self.disabled.is_none()
            && self.model.is_none()
    }
}

/// One daemon publishes these, and its sessions are the only writers of this slice, so a process-wide
/// lock around the read and the replace is enough to keep two exits from dropping each other's keys.
fn writeback_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `Some` when `account_config`'s source entry is a symlink to a file that exists. A plain file is
/// the account's own, and a dangling link has no shared file to write.
fn shared_config_writeback(
    source_home: &Path,
    grok_home: &Path,
    launch_model: Option<&str>,
) -> Option<SharedConfigWriteback> {
    let account_config = source_home.join("config.toml");
    let meta = std::fs::symlink_metadata(&account_config).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    std::fs::canonicalize(&account_config).ok()?;
    let session_config = grok_home.join("config.toml");
    let text = std::fs::read_to_string(&session_config).ok()?;
    let table: toml::Table = text.parse().ok()?;
    Some(SharedConfigWriteback {
        session_config,
        account_config,
        baseline: ConfigSlice::from_table(&table),
        launch_model: launch_model.map(str::to_owned),
    })
}

impl SharedConfigWriteback {
    /// Copies the keys the session changed onto the shared file. A failure is logged and leaves the
    /// shared file as it was; the session has already exited, and removing its home must still happen.
    pub fn publish(&self) {
        let _guard = writeback_lock();
        if let Err(err) = self.publish_locked() {
            tracing::warn!(
                path = %self.account_config.display(),
                %err,
                "writing Grok settings back to the shared configuration failed"
            );
        }
    }

    fn publish_locked(&self) -> Result<()> {
        let meta = std::fs::symlink_metadata(&self.account_config)
            .with_context(|| format!("reading {}", self.account_config.display()))?;
        if !meta.file_type().is_symlink() {
            return Ok(());
        }
        let target = std::fs::canonicalize(&self.account_config)
            .with_context(|| format!("resolving {}", self.account_config.display()))?;
        if target == self.session_config {
            anyhow::bail!("the shared configuration resolves to the session's own copy");
        }
        if std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink()) {
            anyhow::bail!("the shared configuration did not resolve to a file");
        }

        let session: toml::Table = std::fs::read_to_string(&self.session_config)
            .with_context(|| format!("reading {}", self.session_config.display()))?
            .parse()
            .context("the session's configuration is not valid TOML")?;
        let changes =
            ConfigSlice::from_table(&session).changes(&self.baseline, self.launch_model.as_deref());
        if changes.is_empty() {
            return Ok(());
        }

        let current = std::fs::read_to_string(&target)
            .with_context(|| format!("reading {}", target.display()))?;
        let merged = apply_changes(&current, &changes)?;
        if merged == current {
            return Ok(());
        }
        let before: toml::Table = current
            .parse()
            .context("the shared configuration is not valid TOML")?;
        let after: toml::Table = merged
            .parse()
            .context("the shared configuration would be left unreadable")?;
        let mut expected = before.clone();
        apply_to_table(&mut expected, &changes)
            .context("the shared configuration is not shaped so these keys can be set")?;
        if after != expected {
            anyhow::bail!(
                "writing the keys would change more of the shared configuration than those keys"
            );
        }
        crate::trust::write_replacing(&target, &merged)
    }
}

impl ConfigSlice {
    fn from_table(table: &toml::Table) -> Self {
        Self {
            theme: string_key(table, "ui", "theme"),
            enabled: string_array(table, "plugins", "enabled"),
            disabled: string_array(table, "plugins", "disabled"),
            model: string_key(table, "models", "default"),
        }
    }

    /// What of `self` (the session file at exit) differs from `baseline` (the copy taken before
    /// Grok ran) and is safe to publish. A `[models] default` equal to `launch_model` is the launch
    /// flag's own write.
    fn changes(&self, baseline: &Self, launch_model: Option<&str>) -> ConfigChanges {
        ConfigChanges {
            theme: self.theme.clone().filter(|_| self.theme != baseline.theme),
            enabled: self
                .enabled
                .clone()
                .filter(|_| self.enabled != baseline.enabled),
            disabled: self
                .disabled
                .clone()
                .filter(|_| self.disabled != baseline.disabled),
            model: self.model.clone().filter(|value| {
                self.model != baseline.model && launch_model.is_none_or(|launch| value != launch)
            }),
        }
    }
}

fn string_key(table: &toml::Table, section: &str, key: &str) -> Option<String> {
    table
        .get(section)?
        .as_table()?
        .get(key)?
        .as_str()
        .map(str::to_owned)
}

/// `None` when the key is absent or any element is not a string, so a shape this does not know is
/// left untouched rather than rewritten as a string list.
fn string_array(table: &toml::Table, section: &str, key: &str) -> Option<Vec<String>> {
    let array = table.get(section)?.as_table()?.get(key)?.as_array()?;
    let mut items = Vec::with_capacity(array.len());
    for value in array {
        items.push(value.as_str()?.to_owned());
    }
    Some(items)
}

fn apply_changes(current: &str, changes: &ConfigChanges) -> Result<String> {
    let mut doc = current
        .parse::<toml_edit::DocumentMut>()
        .context("the shared configuration is not valid TOML")?;
    if let Some(theme) = &changes.theme {
        set_value(&mut doc, "ui", "theme", theme.as_str().into())?;
    }
    if let Some(enabled) = &changes.enabled {
        set_value(&mut doc, "plugins", "enabled", string_array_value(enabled))?;
    }
    if let Some(disabled) = &changes.disabled {
        set_value(
            &mut doc,
            "plugins",
            "disabled",
            string_array_value(disabled),
        )?;
    }
    if let Some(model) = &changes.model {
        set_value(&mut doc, "models", "default", model.as_str().into())?;
    }
    Ok(doc.to_string())
}

fn string_array_value(items: &[String]) -> toml_edit::Value {
    let mut array = toml_edit::Array::new();
    for item in items {
        array.push(item.as_str());
    }
    toml_edit::Value::Array(array)
}

/// Sets `section.key` in a standard table or an inline table. Refuses when `section` is some other
/// kind of value, rather than replacing it.
fn set_value(
    doc: &mut toml_edit::DocumentMut,
    section: &str,
    key: &str,
    value: toml_edit::Value,
) -> Result<()> {
    let item = doc
        .as_table_mut()
        .entry(section)
        .or_insert(toml_edit::Item::None);
    if item.is_none() {
        *item = toml_edit::table();
    }
    if let Some(table) = item.as_table_mut() {
        table.insert(key, toml_edit::value(value));
        return Ok(());
    }
    if let Some(inline) = item
        .as_value_mut()
        .and_then(|value| value.as_inline_table_mut())
    {
        inline.insert(key, value);
        return Ok(());
    }
    anyhow::bail!("`[{section}]` is not a table, so `{key}` was not written")
}

fn apply_to_table(table: &mut toml::Table, changes: &ConfigChanges) -> Result<()> {
    if let Some(theme) = &changes.theme {
        set_table_string(table, "ui", "theme", theme.clone())?;
    }
    if let Some(enabled) = &changes.enabled {
        set_table_array(table, "plugins", "enabled", enabled)?;
    }
    if let Some(disabled) = &changes.disabled {
        set_table_array(table, "plugins", "disabled", disabled)?;
    }
    if let Some(model) = &changes.model {
        set_table_string(table, "models", "default", model.clone())?;
    }
    Ok(())
}

fn section_table<'a>(table: &'a mut toml::Table, section: &str) -> Result<&'a mut toml::Table> {
    let value = table
        .entry(section)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    value
        .as_table_mut()
        .with_context(|| format!("`[{section}]` is not a table"))
}

fn set_table_string(
    table: &mut toml::Table,
    section: &str,
    key: &str,
    value: String,
) -> Result<()> {
    section_table(table, section)?.insert(key.to_owned(), toml::Value::String(value));
    Ok(())
}

fn set_table_array(
    table: &mut toml::Table,
    section: &str,
    key: &str,
    items: &[String],
) -> Result<()> {
    let array = items.iter().cloned().map(toml::Value::String).collect();
    section_table(table, section)?.insert(key.to_owned(), toml::Value::Array(array));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::{spec_fixture, SpecFixture};

    #[test]
    fn terminal_titles_lose_the_spinner_the_status_and_groks_own_name() {
        let task = |raw: &str| GrokAdapter.task_title(raw, &[]);
        for raw in [
            "grok",
            "⠋ - Waiting for response… - grok",
            "- Waiting for response… - grok",
            "",
        ] {
            assert_eq!(task(raw), None, "{raw}");
        }
        let title = "Explain in one sentence what a pseudo te…";
        assert_eq!(
            task(&format!("⠋ - Waiting for response… - {title} - grok")).as_deref(),
            Some(title)
        );
        assert_eq!(
            task(&format!("- Waiting for response… - {title} - grok")).as_deref(),
            Some(title),
            "the spinner may already be taken off"
        );
        assert_eq!(task(&format!("{title} - grok")).as_deref(), Some(title));
        assert_eq!(
            task("⠋ - Fix the build - grok").as_deref(),
            Some("Fix the build"),
            "an item that is no status stays"
        );
        assert_eq!(
            task("⠋ - Explain what a pseudo te… - grok").as_deref(),
            Some("Explain what a pseudo te…"),
            "a lone truncated title is not a status"
        );
    }

    /// `plan` against an empty system-wide configuration directory of the fixture's own.
    fn plan_isolated(spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        plan_under(spec, &spec.scratch.join("etc-grok"))
    }

    /// Stands a fake `~/.grok` up inside the fixture and points the launch environment at it.
    fn with_default_home(fixture: &mut SpecFixture) -> PathBuf {
        let home = fixture.scratch.join("user-grok-home");
        std::fs::create_dir_all(home.join("sessions")).expect("sessions directory");
        std::fs::write(home.join("config.toml"), "default_model = \"x\"\n").expect("config");
        std::fs::write(home.join("auth.json"), "{}").expect("auth");
        std::fs::write(home.join("managed_config.toml"), "").expect("managed config");
        fixture
            .shell_env
            .insert("GROK_HOME".to_string(), home.to_string_lossy().into_owned());
        home
    }

    /// The terminal's appearance chooses Grok's theme. With none sent, Grok follows the operating
    /// system rather than its compiled dark default.
    #[test]
    fn the_terminal_appearance_sets_grok_theme() {
        let theme = |appearance| {
            let mut fixture = spec_fixture();
            with_default_home(&mut fixture);
            fixture.terminal_appearance = appearance;
            plan_isolated(&fixture.spec())
                .expect("plan")
                .env
                .into_iter()
                .find(|(key, _)| key == "GROK_THEME")
                .expect("GROK_THEME")
                .1
        };
        assert_eq!(theme(None), "auto");
        assert_eq!(theme(Some(TerminalAppearance::Light)), "grokday");
        assert_eq!(theme(Some(TerminalAppearance::Dark)), "groknight");
    }

    /// With no model or effort set, neither flag is passed, so Grok keeps its own choice.
    #[test]
    fn no_model_or_effort_adds_no_flags() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        for flag in ["--model", "--reasoning-effort"] {
            assert!(!plan.args.iter().any(|arg| arg == flag), "{flag}");
        }
    }

    #[test]
    fn a_model_and_effort_come_after_the_rules_and_before_the_task() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        fixture.model = Some("grok-4".to_string());
        fixture.effort = Some("low".to_string());
        let mut spec = fixture.spec();
        spec.task = Some("do it");
        let plan = plan_isolated(&spec).expect("plan");
        let rules = plan.args.iter().position(|arg| arg == "--rules").unwrap();
        assert_eq!(
            plan.args[rules + 2..],
            ["--model", "grok-4", "--reasoning-effort", "low", "do it"]
        );
    }

    #[test]
    fn a_pinned_grok_home_replaces_the_users_as_the_source_of_the_farm() {
        let mut fixture = spec_fixture();
        let shell_home = with_default_home(&mut fixture);
        std::fs::write(
            shell_home.join("trusted_folders.toml"),
            "[folders.\"/from/shell\"]\ntrusted = true\n",
        )
        .expect("trust store");
        let pinned = fixture.scratch.join("grok-alt");
        std::fs::create_dir_all(pinned.join("sessions")).expect("sessions directory");
        std::fs::write(pinned.join("config.toml"), "default_model = \"pinned\"\n").expect("config");
        std::fs::write(pinned.join("auth.json"), "{\"pinned\":true}").expect("auth");
        std::fs::write(
            pinned.join("trusted_folders.toml"),
            "[folders.\"/from/pinned\"]\ntrusted = true\n",
        )
        .expect("trust store");
        fixture.config_dir = Some(pinned.clone());

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        // Still Octoboard's own farm that Grok runs against, never the chosen directory itself.
        assert_eq!(
            plan.env
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            ["GROK_HOME", "GROK_THEME"]
        );
        assert!(farm.starts_with(&*fixture.scratch), "{}", farm.display());
        // Grok persists into `config.toml`, so it stays a copy and never a link into the source.
        assert!(farm.join("config.toml").is_file());
        assert!(!farm.join("config.toml").is_symlink());
        let config = std::fs::read_to_string(farm.join("config.toml")).expect("the copy");
        assert!(config.contains("default_model = \"pinned\""), "{config}");
        assert!(!config.contains("default_model = \"x\""), "{config}");
        let trust = std::fs::read_to_string(farm.join("trusted_folders.toml")).expect("the copy");
        assert!(trust.contains("/from/pinned"), "{trust}");
        assert!(!trust.contains("/from/shell"), "{trust}");
        // Login and session records are the chosen directory's, so a resume finds its transcript.
        assert_eq!(
            std::fs::read_link(farm.join("auth.json")).expect("a link"),
            std::fs::canonicalize(pinned.join("auth.json")).expect("canonical path")
        );
        assert_eq!(
            std::fs::read_link(farm.join("sessions")).expect("a link"),
            std::fs::canonicalize(pinned.join("sessions")).expect("canonical path")
        );
    }

    #[test]
    fn a_pinned_grok_home_that_has_gone_refuses_resuming_a_conversation() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some("agent-side-id");
        let err = plan_isolated(&spec).err().expect("refused");
        let message = err.to_string();
        assert!(message.contains("Grok"), "{message}");
        assert!(message.contains("missing"), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }

    /// Unlike Claude Code and Codex, a fresh Grok session pinned to a directory that does not exist
    /// is refused too, not only a resumed one: Grok cannot safely create a home the way the other
    /// two can, so there is no "let the agent create it" case for a missing pinned source home —
    /// it fails `require_initialized_grok_home` instead of the generic vanished-directory check.
    #[test]
    fn a_pinned_grok_home_that_has_gone_refuses_a_fresh_launch_too() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        let err = plan_isolated(&fixture.spec()).err().expect("refused");
        assert_eq!(
            err.downcast_ref::<crate::protocol::CodedError>()
                .expect("coded")
                .code,
            crate::protocol::error_code::GROK_HOME_NOT_INITIALIZED
        );
    }

    /// A pinned directory that exists but Grok has never been run against carries no login and no
    /// session history for the per-session farm to link, so it is refused rather than silently
    /// handed to Grok, which would write a login and a conversation into the throw-away farm and
    /// lose both.
    #[test]
    fn a_pinned_grok_home_that_was_never_initialized_refuses_the_launch() {
        let mut fixture = spec_fixture();
        let empty = fixture.scratch.join("never-run");
        std::fs::create_dir_all(&empty).expect("empty directory");
        fixture.config_dir = Some(empty.clone());
        let err = plan_isolated(&fixture.spec()).err().expect("refused");
        let message = err.to_string();
        assert!(message.contains("never-run"), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }

    /// The default home is never held to the same requirement: it is the user's own existing
    /// setup, not an account Octoboard is minting, so an uninitialized one is still left to Grok.
    /// Unlike `with_default_home`, the home here is stripped to an empty directory — no
    /// `sessions/`, no `auth.json` — so the check is actually exercised rather than skipped by
    /// construction, and the test stays off the machine's real `~/.grok`: leaving `GROK_HOME`
    /// unset would fall through to `default_grok_home`'s `home_dir().join(".grok")`, which this
    /// test does not own and cannot assume either way about.
    #[test]
    fn the_default_home_is_not_checked_for_initialization() {
        let mut fixture = spec_fixture();
        let home = fixture.scratch.join("user-grok-home");
        std::fs::create_dir_all(&home).expect("empty home directory");
        fixture
            .shell_env
            .insert("GROK_HOME".to_string(), home.to_string_lossy().into_owned());
        plan_isolated(&fixture.spec()).expect("not refused");
    }

    #[test]
    fn the_farm_links_the_users_home_but_owns_the_files_grok_writes_back_to() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let plan = plan_isolated(&fixture.spec()).expect("plan");

        let farm = PathBuf::from(&plan.env[0].1);
        assert_eq!(plan.env[0].0, "GROK_HOME");

        // Login state and session records stay shared with the user's own `grok`.
        assert!(farm.join("auth.json").is_symlink());
        assert!(farm.join("sessions").is_symlink());
        // Grok persists session settings into `config.toml`, so against a symlink that write would
        // reach the user's own file.
        assert!(!farm.join("config.toml").is_symlink());
        assert!(std::fs::read_to_string(farm.join("config.toml"))
            .expect("the copy")
            .starts_with(
                &std::fs::read_to_string(home.join("config.toml")).expect("the original")
            ));
        // The deployment sync deletes these at runtime, so the farm must not carry them.
        assert!(!farm.join("managed_config.toml").exists());
        assert!(!farm.join("requirements.toml").exists());
    }

    /// The farm's trust store is the user's, copied as it is: the project is not written into it,
    /// so Grok shows its own confirmation for a folder the user has not trusted, and the plan names
    /// where the entry Grok writes on a press is carried from and to.
    #[test]
    fn the_farms_trust_store_is_the_users_copied_without_the_project_added() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let users = "[folders.\"/somewhere/else\"]\ntrusted = true\n";
        std::fs::write(home.join(TRUST_STORE), users).expect("trust store");

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        assert!(!farm.join(TRUST_STORE).is_symlink());
        assert_eq!(
            std::fs::read_to_string(farm.join(TRUST_STORE)).expect("the copy"),
            users
        );
        let carried = plan.carried_trust.expect("Grok's trust is carried");
        assert_eq!(carried.session_store, farm.join(TRUST_STORE));
        assert_eq!(carried.user_store, home.join(TRUST_STORE));
        let canonical = std::fs::canonicalize(&fixture.cwd).expect("canonical path");
        assert_eq!(carried.folder, canonical.to_string_lossy());
    }

    #[test]
    fn the_mcp_server_is_appended_to_the_farms_config_without_losing_the_users_own() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        let config = std::fs::read_to_string(farm.join("config.toml")).expect("the copy");
        assert!(
            config.contains("default_model = \"x\""),
            "the user's own configuration survives"
        );
        assert!(config.contains("[mcp_servers.octoboard]"));
        assert!(config.contains(&fixture.self_exe));
        assert!(config.contains(&fixture.mcp_token));
    }

    /// Grok cancels a tool call it gives up on, and a cancellation withdraws a waiting request for
    /// a console session, so its limit is set above Octoboard's forwarding limit, for this server
    /// only. The ordering itself is asserted where the value is defined.
    #[test]
    fn the_servers_tool_timeout_sits_above_octoboards_own_call_limit() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        let config = std::fs::read_to_string(farm.join("config.toml")).expect("the copy");
        // Read out of Octoboard's own table rather than from anywhere in the file: emitted above
        // its header the key would be a top-level one, setting the limit for every MCP server the
        // user has.
        let table = config
            .split(&format!("[mcp_servers.{}]\n", mcp::SERVER_KEY))
            .nth(1)
            .expect("Octoboard's table")
            .split("\n[")
            .next()
            .expect("its end");
        let emitted: u64 = table
            .lines()
            .find_map(|line| line.strip_prefix("tool_timeout_sec = "))
            .expect("the limit")
            .parse()
            .expect("whole seconds");
        assert_eq!(emitted, 660);
    }

    /// The role text travels in `--rules`, which is passed for every session, not just a console
    /// session's.
    #[test]
    fn the_role_text_travels_in_rules() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let at = plan
            .args
            .iter()
            .position(|arg| arg == "--rules")
            .expect("--rules is passed");
        assert_eq!(
            plan.args[at + 1],
            mcp::role::role_description(fixture.role, fixture.owner.as_ref(), Agent::Grok)
        );
    }

    #[test]
    fn the_hooks_are_command_type_with_an_explicit_timeout() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        let document: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(farm.join("hooks").join("octoboard.json")).expect("hooks"),
        )
        .expect("the hooks file is JSON");
        let hooks = document["hooks"].as_object().expect("a hooks table");
        assert_eq!(hooks.len(), HOOK_EVENTS.len());
        for event in HOOK_EVENTS {
            let handler = &hooks[*event][0]["hooks"][0];
            // HTTP hooks cannot reach the daemon at all: Grok's SSRF protection rejects plain HTTP
            // and private addresses alike.
            assert_eq!(handler["type"], "command");
            // Grok defaults `Stop`, `SubagentStop` and `PostToolUse` to 600 s, which would stall
            // the UI for ten minutes on a wedged hook.
            assert_eq!(handler["timeout"], HOOK_TIMEOUT_SECS);
        }
    }

    #[test]
    fn the_entries_grok_protects_under_its_sandbox_are_real_copies() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::create_dir_all(home.join("bundled/nested")).expect("bundled");
        std::fs::write(home.join("bundled/nested/tool"), "tool").expect("bundled file");
        std::fs::write(home.join("bundled/top"), "top").expect("bundled file");
        std::fs::create_dir_all(home.join("bundles")).expect("bundles");
        std::fs::write(home.join("hooks-paths"), "/some/hooks\n").expect("hooks-paths");
        std::fs::write(home.join("trusted_folders.toml"), "").expect("trust store");

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);

        for name in ["bundled", "bundles", "hooks-paths"] {
            assert!(!farm.join(name).is_symlink(), "{name}");
        }
        assert!(farm.join("bundled").is_dir());
        assert!(farm.join("bundles").is_dir());
        assert_eq!(
            std::fs::read_to_string(farm.join("bundled/nested/tool")).expect("nested copy"),
            "tool"
        );
        assert_eq!(
            std::fs::read_to_string(farm.join("bundled/top")).expect("copy"),
            "top"
        );
        assert_eq!(
            std::fs::read_to_string(farm.join("hooks-paths")).expect("copy"),
            "/some/hooks\n"
        );
        // Absent in the source stays absent, for Grok to create.
        assert!(!farm.join("sandbox.toml").exists());
        assert!(!farm.join("sandbox.toml").is_symlink());
        // Writing into the copy never reaches the source.
        std::fs::write(farm.join("bundled/top"), "changed").expect("write");
        assert_eq!(
            std::fs::read_to_string(home.join("bundled/top")).expect("source"),
            "top"
        );
        // Everything else is as before.
        assert!(farm.join("auth.json").is_symlink());
        assert!(farm.join("sessions").is_symlink());
        assert!(!farm.join("config.toml").is_symlink());
        assert!(!farm.join(TRUST_STORE).is_symlink());
    }

    /// Grok checks that the entry is a real directory, so a source `bundled` that is a link becomes
    /// one in the farm; a link inside it stays a link rather than being followed.
    #[test]
    fn a_linked_source_directory_becomes_a_real_one_and_inner_links_stay_links() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let elsewhere = fixture.scratch.join("elsewhere");
        std::fs::create_dir_all(elsewhere.join("sub")).expect("target directory");
        std::fs::write(elsewhere.join("sub/tool"), "tool").expect("file");
        std::os::unix::fs::symlink("sub/tool", elsewhere.join("inner")).expect("inner link");
        std::os::unix::fs::symlink(&elsewhere, home.join("bundled")).expect("link");

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);

        assert!(!farm.join("bundled").is_symlink());
        assert!(farm.join("bundled").is_dir());
        assert_eq!(
            std::fs::read_to_string(farm.join("bundled/sub/tool")).expect("copy"),
            "tool"
        );
        assert_eq!(
            std::fs::read_link(farm.join("bundled/inner")).expect("still a link"),
            Path::new("sub/tool")
        );
    }

    /// A sharing account's entries are links into the default directory; the farm links straight to
    /// the final target rather than through the account's link, however long the chain.
    #[test]
    fn a_source_entry_that_is_a_link_is_reached_through_one_link() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let default = fixture.scratch.join("default-home");
        std::fs::create_dir_all(default.join("skills")).expect("default directory");
        std::fs::write(default.join("AGENTS.md"), "shared").expect("default file");
        std::os::unix::fs::symlink(default.join("AGENTS.md"), home.join("AGENTS.md"))
            .expect("file link");
        std::os::unix::fs::symlink(default.join("skills"), home.join("skills"))
            .expect("directory link");
        let middle = fixture.scratch.join("middle");
        std::os::unix::fs::symlink(default.join("AGENTS.md"), &middle).expect("first hop");
        std::os::unix::fs::symlink(&middle, home.join("chained.md")).expect("second hop");

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);

        let file = std::fs::canonicalize(default.join("AGENTS.md")).expect("canonical path");
        assert_eq!(
            std::fs::read_link(farm.join("AGENTS.md")).expect("a link"),
            file
        );
        assert_eq!(
            std::fs::read_link(farm.join("chained.md")).expect("a link"),
            file
        );
        assert_eq!(
            std::fs::read_link(farm.join("skills")).expect("a link"),
            std::fs::canonicalize(default.join("skills")).expect("canonical path")
        );
        assert_eq!(
            std::fs::read_to_string(farm.join("AGENTS.md")).expect("read"),
            "shared"
        );
    }

    #[test]
    fn a_plain_source_entry_is_linked_to_itself() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::write(home.join("AGENTS.md"), "own").expect("file");

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);

        assert_eq!(
            std::fs::read_link(farm.join("AGENTS.md")).expect("a link"),
            std::fs::canonicalize(home.join("AGENTS.md")).expect("canonical path")
        );
    }

    /// With nothing to resolve to, the farm link keeps pointing at the source entry.
    #[test]
    fn a_dangling_source_link_is_linked_to_the_source_entry() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::os::unix::fs::symlink(fixture.scratch.join("gone"), home.join("AGENTS.md"))
            .expect("dangling link");

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);

        assert_eq!(
            std::fs::read_link(farm.join("AGENTS.md")).expect("a link"),
            home.join("AGENTS.md")
        );
    }

    /// Copies are taken from the source entries' contents, so links in the source change nothing
    /// about which entries are real.
    #[test]
    fn linked_source_entries_still_yield_real_copies_of_the_owned_files() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let default = fixture.scratch.join("default-home");
        std::fs::create_dir_all(default.join("bundles")).expect("default directory");
        std::fs::write(default.join("bundles/one"), "1").expect("bundle");
        std::fs::write(default.join("hooks-paths"), "/h\n").expect("hooks-paths");
        std::fs::write(default.join("sandbox.toml"), "[profiles.p]\n").expect("sandbox");
        std::fs::write(default.join(TRUST_STORE), "[folders.\"/t\"]\n").expect("trust");
        std::fs::remove_file(home.join("config.toml")).expect("replace config");
        std::fs::write(default.join("config.toml"), "default_model = \"d\"\n").expect("config");
        for name in [
            "bundles",
            "hooks-paths",
            "sandbox.toml",
            TRUST_STORE,
            "config.toml",
        ] {
            std::os::unix::fs::symlink(default.join(name), home.join(name)).expect("link");
        }

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);

        for name in [
            "bundles",
            "hooks-paths",
            "sandbox.toml",
            TRUST_STORE,
            "config.toml",
        ] {
            assert!(!farm.join(name).is_symlink(), "{name}");
        }
        assert_eq!(
            std::fs::read_to_string(farm.join("bundles/one")).expect("copy"),
            "1"
        );
        assert_eq!(
            std::fs::read_to_string(farm.join("hooks-paths")).expect("copy"),
            "/h\n"
        );
        assert_eq!(
            std::fs::read_to_string(farm.join("sandbox.toml")).expect("copy"),
            "[profiles.p]\n"
        );
        assert_eq!(
            std::fs::read_to_string(farm.join(TRUST_STORE)).expect("copy"),
            "[folders.\"/t\"]\n"
        );
        assert!(std::fs::read_to_string(farm.join("config.toml"))
            .expect("copy")
            .contains("default_model = \"d\""));
    }

    #[test]
    fn a_sandbox_profile_file_is_copied_too() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::write(home.join("sandbox.toml"), "[profiles.p]\n").expect("sandbox.toml");
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        assert!(!farm.join("sandbox.toml").is_symlink());
        assert_eq!(
            std::fs::read_to_string(farm.join("sandbox.toml")).expect("copy"),
            "[profiles.p]\n"
        );
    }

    /// Plans the launch and returns what refused it, `(source, profile)`, or `None` when it was not
    /// refused. A refusal is also asserted to carry `GROK_SANDBOX_UNSUPPORTED` and to leave no farm.
    fn refused_profile(fixture: &SpecFixture, resume: Option<&str>) -> Option<(String, String)> {
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = resume;
        let farm = spec.scratch.join("grok-home");
        std::fs::remove_dir_all(&farm).ok();
        let result = plan_isolated(&spec);
        match result {
            Ok(_) => None,
            Err(err) => {
                let coded = err.downcast_ref::<CodedError>().expect("coded");
                assert_eq!(coded.code, error_code::GROK_SANDBOX_UNSUPPORTED);
                assert!(!farm.exists(), "a refusal builds no farm");
                Some((
                    coded.params["source"].clone(),
                    coded.params["profile"].clone(),
                ))
            }
        }
    }

    /// Writes the `summary.json` Grok keeps for conversation `id` with `profile` as its saved
    /// sandbox profile, under a working-directory-named folder of the home's `sessions/`.
    fn save_session_profile(home: &Path, id: &str, profile: &str) {
        let dir = home.join("sessions/%2Fsome%2Fcwd").join(id);
        std::fs::create_dir_all(&dir).expect("session directory");
        std::fs::write(
            dir.join("summary.json"),
            format!("{{\"sandbox_profile\":\"{profile}\"}}"),
        )
        .expect("summary");
    }

    const SAVED_ON: &str = "11111111-1111-4111-8111-111111111111";
    const SAVED_OFF: &str = "22222222-2222-4222-8222-222222222222";
    const NEVER_SAVED: &str = "33333333-3333-4333-8333-333333333333";

    #[test]
    fn a_launch_with_no_sandbox_anywhere_is_not_refused() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        assert_eq!(refused_profile(&fixture, None), None);
    }

    #[test]
    fn a_sandbox_in_the_launch_environment_refuses_the_launch() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        fixture
            .shell_env
            .insert("GROK_SANDBOX".to_string(), "workspace".to_string());
        assert_eq!(
            refused_profile(&fixture, None),
            Some(("GROK_SANDBOX".to_string(), "workspace".to_string()))
        );
        fixture
            .shell_env
            .insert("GROK_SANDBOX".to_string(), "off".to_string());
        assert_eq!(refused_profile(&fixture, None), None);
    }

    #[test]
    fn the_sandbox_profile_of_the_users_config_refuses_unless_it_is_off() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::write(
            home.join("config.toml"),
            "[sandbox]\nprofile = \"strict\"\n",
        )
        .expect("config");
        assert_eq!(
            refused_profile(&fixture, None),
            Some((
                home.join("config.toml").to_string_lossy().into_owned(),
                "strict".to_string()
            ))
        );
        std::fs::write(home.join("config.toml"), "[sandbox]\nprofile = \"off\"\n").expect("config");
        assert_eq!(refused_profile(&fixture, None), None);
    }

    #[test]
    fn an_off_environment_outranks_a_sandbox_in_the_users_config() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::write(
            home.join("config.toml"),
            "[sandbox]\nprofile = \"workspace\"\n",
        )
        .expect("config");
        fixture
            .shell_env
            .insert("GROK_SANDBOX".to_string(), "off".to_string());
        assert_eq!(refused_profile(&fixture, None), None);
    }

    #[test]
    fn a_config_that_is_not_toml_is_ignored() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::write(home.join("config.toml"), "[sandbox\nprofile = ").expect("config");
        assert_eq!(refused_profile(&fixture, None), None);
    }

    #[test]
    fn the_environment_outranks_the_users_config_and_the_config_outranks_the_managed_default() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let system = fixture.scratch.join("etc-grok");
        std::fs::create_dir_all(&system).expect("system directory");
        std::fs::write(
            system.join("managed_config.toml"),
            "[sandbox]\nprofile = \"workspace\"\n",
        )
        .expect("managed config");
        let check = |fixture: &SpecFixture| {
            sandbox_setting(&fixture.spec(), &home, &system).map(|found| found.profile)
        };
        assert_eq!(
            check(&fixture).as_deref(),
            Some("workspace"),
            "a managed default with no user setting"
        );
        std::fs::write(home.join("config.toml"), "[sandbox]\nprofile = \"off\"\n").expect("config");
        assert_eq!(check(&fixture), None, "the user's config wins");
        fixture
            .shell_env
            .insert("GROK_SANDBOX".to_string(), "strict".to_string());
        assert_eq!(
            check(&fixture).as_deref(),
            Some("strict"),
            "the environment wins"
        );
    }

    /// The home's own `requirements.toml` and `managed_config.toml` are excluded from the farm, so
    /// Grok never loads them and they refuse nothing.
    #[test]
    fn the_homes_own_requirements_and_managed_config_are_not_read() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        for name in ["requirements.toml", "managed_config.toml"] {
            std::fs::write(home.join(name), "[sandbox]\nprofile = \"workspace\"\n").expect(name);
        }
        assert_eq!(refused_profile(&fixture, None), None);
    }

    #[test]
    fn a_system_wide_requirements_pin_refuses_whatever_else_is_set() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let system = fixture.scratch.join("etc-grok");
        std::fs::create_dir_all(&system).expect("system directory");
        std::fs::write(
            system.join("requirements.toml"),
            "[sandbox]\nprofile = \"workspace\"\n",
        )
        .expect("requirements");
        fixture
            .shell_env
            .insert("GROK_SANDBOX".to_string(), "off".to_string());
        assert_eq!(
            refused_profile(&fixture, None),
            Some((
                system
                    .join("requirements.toml")
                    .to_string_lossy()
                    .into_owned(),
                "workspace".to_string()
            ))
        );
    }

    #[test]
    fn a_system_wide_requirements_pin_and_managed_default_are_read() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let system = fixture.scratch.join("etc-grok");
        std::fs::create_dir_all(&system).expect("system directory");
        let check = |fixture: &SpecFixture| {
            sandbox_setting(&fixture.spec(), &home, &system).map(|found| found.profile)
        };
        assert_eq!(check(&fixture), None);

        std::fs::write(
            system.join("managed_config.toml"),
            "[sandbox]\nprofile = \"read-only\"\n",
        )
        .expect("managed config");
        assert_eq!(check(&fixture).as_deref(), Some("read-only"));

        // A requirements pin of `off` outranks a sandbox set anywhere below it.
        std::fs::write(
            system.join("requirements.toml"),
            "[sandbox]\nprofile = \"off\"\n",
        )
        .expect("requirements");
        assert_eq!(check(&fixture), None);
        std::fs::write(
            system.join("requirements.toml"),
            "[sandbox]\nprofile = \"custom\"\n",
        )
        .expect("requirements");
        assert_eq!(check(&fixture).as_deref(), Some("custom"));
    }

    #[test]
    fn a_resumed_session_follows_the_profile_saved_with_it() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        save_session_profile(&home, SAVED_ON, "workspace");
        save_session_profile(&home, SAVED_OFF, "off");

        let saved = home
            .join("sessions/%2Fsome%2Fcwd")
            .join(SAVED_ON)
            .join("summary.json");
        assert_eq!(
            refused_profile(&fixture, Some(SAVED_ON)),
            Some((
                saved.to_string_lossy().into_owned(),
                "workspace".to_string()
            ))
        );
        assert_eq!(refused_profile(&fixture, Some(SAVED_OFF)), None);
        // A conversation with nothing saved falls back to the rules for a new session.
        assert_eq!(refused_profile(&fixture, Some(NEVER_SAVED)), None);

        // The saved profile decides over a configuration that says otherwise ...
        std::fs::write(
            home.join("config.toml"),
            "[sandbox]\nprofile = \"workspace\"\n",
        )
        .expect("config");
        assert_eq!(refused_profile(&fixture, Some(SAVED_OFF)), None);
        assert!(refused_profile(&fixture, Some(NEVER_SAVED)).is_some());
        // ... but not over a requirements pin.
        let system = fixture.scratch.join("etc-grok");
        std::fs::create_dir_all(&system).expect("system directory");
        std::fs::write(
            system.join("requirements.toml"),
            "[sandbox]\nprofile = \"strict\"\n",
        )
        .expect("requirements");
        assert!(refused_profile(&fixture, Some(SAVED_OFF)).is_some());
    }

    /// Grok refuses a resume whose requested profile differs from the saved one, so the launch
    /// environment counts on a resume even when the saved profile is `off`.
    #[test]
    fn on_a_resume_either_the_saved_profile_or_the_environment_refuses() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        save_session_profile(&home, SAVED_ON, "workspace");
        save_session_profile(&home, SAVED_OFF, "off");

        fixture
            .shell_env
            .insert("GROK_SANDBOX".to_string(), "workspace".to_string());
        assert_eq!(
            refused_profile(&fixture, Some(SAVED_OFF)),
            Some(("GROK_SANDBOX".to_string(), "workspace".to_string()))
        );
        fixture
            .shell_env
            .insert("GROK_SANDBOX".to_string(), "off".to_string());
        assert_eq!(
            refused_profile(&fixture, Some(SAVED_ON)).map(|found| found.1),
            Some("workspace".to_string())
        );
        assert_eq!(refused_profile(&fixture, Some(SAVED_OFF)), None);
    }

    /// The id becomes a path component, so one that is not the shape of a Grok conversation id is
    /// not looked up at all.
    #[test]
    fn an_implausible_resume_id_is_ignored_not_read() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let outside = fixture.scratch.join("outside");
        std::fs::create_dir_all(&outside).expect("outside directory");
        std::fs::write(
            outside.join("summary.json"),
            "{\"sandbox_profile\":\"workspace\"}",
        )
        .expect("summary");
        // Reached from a folder under `sessions/` as `../x`, and by an absolute path.
        std::fs::create_dir_all(home.join("sessions/%2Fsome%2Fcwd")).expect("sessions folder");
        std::fs::create_dir_all(home.join("sessions/x")).expect("x");
        std::fs::write(
            home.join("sessions/x/summary.json"),
            "{\"sandbox_profile\":\"workspace\"}",
        )
        .expect("summary");
        for id in ["../x".to_string(), outside.to_string_lossy().into_owned()] {
            assert_eq!(saved_sandbox_profile(&home, &id), None, "{id}");
            assert_eq!(refused_profile(&fixture, Some(&id)), None, "{id}");
        }
    }

    #[test]
    fn a_summary_that_is_a_link_is_not_read() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let target = fixture.scratch.join("elsewhere.json");
        std::fs::write(&target, "{\"sandbox_profile\":\"workspace\"}").expect("target");
        let dir = home.join("sessions/%2Fsome%2Fcwd").join(SAVED_ON);
        std::fs::create_dir_all(&dir).expect("session directory");
        std::os::unix::fs::symlink(&target, dir.join("summary.json")).expect("link");
        assert_eq!(refused_profile(&fixture, Some(SAVED_ON)), None);
    }

    const SHARED_CONFIG: &str = "\
# kept comment
[models]
default = \"kept-model\" # kept
default_reasoning_effort = \"high\"

[ui]
theme = \"dark\"

[plugins]
enabled = [\"kept-plugin\"]

[notes]
mark = 'stay'
";

    /// Replaces the fixture's source `config.toml` with a link to a file holding `text`.
    fn link_config(fixture: &mut SpecFixture, text: &str) -> PathBuf {
        let account = with_default_home(fixture);
        let shared = fixture.scratch.join("default-home");
        std::fs::create_dir_all(&shared).expect("default directory");
        std::fs::write(shared.join("config.toml"), text).expect("shared config");
        std::fs::remove_file(account.join("config.toml")).expect("replace config");
        std::os::unix::fs::symlink(shared.join("config.toml"), account.join("config.toml"))
            .expect("link");
        shared.join("config.toml")
    }

    fn session_config(plan: &LaunchPlan) -> PathBuf {
        PathBuf::from(&plan.env[0].1).join("config.toml")
    }

    #[test]
    fn a_plain_config_is_not_written_back() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        assert!(plan.shared_config_writeback.is_none());
    }

    #[test]
    fn a_dangling_config_link_is_not_written_back() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::remove_file(home.join("config.toml")).expect("replace config");
        std::os::unix::fs::symlink(
            fixture.scratch.join("missing.toml"),
            home.join("config.toml"),
        )
        .expect("dangling link");
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        assert!(plan.shared_config_writeback.is_none());
        assert!(std::fs::symlink_metadata(home.join("config.toml"))
            .expect("link")
            .file_type()
            .is_symlink());
    }

    #[test]
    fn a_linked_config_gains_the_theme_the_plugin_switches_and_the_chosen_model() {
        use std::os::unix::fs::MetadataExt;

        let mut fixture = spec_fixture();
        fixture.model = Some("launch-model".to_string());
        let shared = link_config(&mut fixture, SHARED_CONFIG);
        let link = PathBuf::from(fixture.shell_env["GROK_HOME"].clone()).join("config.toml");
        let link_ino = std::fs::symlink_metadata(&link).expect("link").ino();
        let link_target = std::fs::read_link(&link).expect("target");
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let session = session_config(&plan);
        assert!(!std::fs::symlink_metadata(&session)
            .expect("copy")
            .file_type()
            .is_symlink());
        assert!(std::fs::read_to_string(&session)
            .expect("copy")
            .contains("[mcp_servers.octoboard]"));
        assert_eq!(
            std::fs::read_to_string(&shared).expect("shared"),
            SHARED_CONFIG
        );
        let writeback = plan.shared_config_writeback.expect("writeback");

        std::fs::write(
            &session,
            "\
[models]
default = \"other-model\"
default_reasoning_effort = \"low\"

[ui]
theme = \"tokyonight\"
max_thoughts_width = 120
yolo = false
compact_mode = false

[plugins]
enabled = []
disabled = [\"kept-plugin\"]

[marketplace]
default_skills_installs_purged = true

[mcp_servers.octoboard]
command = \"/usr/bin/true\"
args = []
tool_timeout_sec = 660
",
        )
        .expect("session config");
        writeback.publish();

        let updated = std::fs::read_to_string(&shared).expect("shared");
        let parsed: toml::Table = updated.parse().expect("shared config");
        assert_eq!(parsed["ui"]["theme"].as_str(), Some("tokyonight"));
        assert_eq!(parsed["models"]["default"].as_str(), Some("other-model"));
        assert_eq!(
            parsed["models"]["default_reasoning_effort"].as_str(),
            Some("high")
        );
        assert_eq!(
            parsed["plugins"]["enabled"]
                .as_array()
                .expect("enabled")
                .len(),
            0
        );
        assert_eq!(
            parsed["plugins"]["disabled"][0].as_str(),
            Some("kept-plugin")
        );
        assert!(parsed.get("marketplace").is_none(), "{updated}");
        assert!(
            parsed["ui"].get("max_thoughts_width").is_none(),
            "{updated}"
        );
        assert!(!updated.contains("mcp_servers"), "{updated}");
        assert!(!updated.contains("launch-model"), "{updated}");
        assert!(updated.contains("# kept comment"), "{updated}");
        assert!(updated.contains("# kept"), "{updated}");
        assert!(updated.contains("mark = 'stay'"), "{updated}");
        let after = std::fs::symlink_metadata(&link).expect("link");
        assert!(after.file_type().is_symlink());
        assert_eq!(after.ino(), link_ino);
        assert_eq!(std::fs::read_link(&link).expect("target"), link_target);
    }

    #[test]
    fn the_launch_model_and_startup_writes_stay_in_the_session_copy() {
        use std::os::unix::fs::MetadataExt;

        let mut fixture = spec_fixture();
        fixture.model = Some("launch-model".to_string());
        let shared = link_config(&mut fixture, SHARED_CONFIG);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let before_ino = std::fs::metadata(&shared).expect("shared").ino();
        std::fs::write(
            session_config(&plan),
            "\
[models]
default = \"launch-model\"
default_reasoning_effort = \"low\"

[ui]
theme = \"dark\"
max_thoughts_width = 120
yolo = false
compact_mode = false

[plugins]
enabled = [\"kept-plugin\"]

[marketplace]
default_skills_installs_purged = true

[mcp_servers.octoboard]
command = \"/usr/bin/true\"
",
        )
        .expect("session config");
        plan.shared_config_writeback.expect("writeback").publish();
        assert_eq!(
            std::fs::read_to_string(&shared).expect("shared"),
            SHARED_CONFIG
        );
        assert_eq!(
            std::fs::metadata(&shared).expect("shared").ino(),
            before_ino
        );
    }

    #[test]
    fn a_model_chosen_without_a_launch_model_is_written_back() {
        let mut fixture = spec_fixture();
        let shared = link_config(&mut fixture, SHARED_CONFIG);
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let mut session = std::fs::read_to_string(session_config(&plan)).expect("copy");
        session = session.replace("kept-model", "other-model");
        std::fs::write(session_config(&plan), session).expect("session config");
        plan.shared_config_writeback.expect("writeback").publish();
        let parsed: toml::Table = std::fs::read_to_string(&shared)
            .expect("shared")
            .parse()
            .expect("shared config");
        assert_eq!(parsed["models"]["default"].as_str(), Some("other-model"));
        assert_eq!(
            parsed["models"]["default_reasoning_effort"].as_str(),
            Some("high")
        );
    }

    #[test]
    fn a_chain_of_links_is_written_at_its_end_and_both_links_stay() {
        use std::os::unix::fs::MetadataExt;

        let mut fixture = spec_fixture();
        let account = with_default_home(&mut fixture);
        let real_dir = fixture.scratch.join("dotfiles");
        std::fs::create_dir_all(&real_dir).expect("dotfiles");
        let real = real_dir.join("config.toml");
        std::fs::write(&real, SHARED_CONFIG).expect("real config");
        let middle = fixture.scratch.join("middle.toml");
        std::os::unix::fs::symlink(&real, &middle).expect("middle link");
        std::fs::remove_file(account.join("config.toml")).expect("replace config");
        std::os::unix::fs::symlink(&middle, account.join("config.toml")).expect("account link");
        let account_ino = std::fs::symlink_metadata(account.join("config.toml"))
            .expect("link")
            .ino();
        let middle_ino = std::fs::symlink_metadata(&middle).expect("link").ino();

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let mut session = std::fs::read_to_string(session_config(&plan)).expect("copy");
        session = session.replace("theme = \"dark\"", "theme = \"tokyonight\"");
        std::fs::write(session_config(&plan), session).expect("session config");
        plan.shared_config_writeback.expect("writeback").publish();

        let parsed: toml::Table = std::fs::read_to_string(&real)
            .expect("real")
            .parse()
            .expect("real config");
        assert_eq!(parsed["ui"]["theme"].as_str(), Some("tokyonight"));
        assert_eq!(
            std::fs::read_link(account.join("config.toml")).expect("account link"),
            middle
        );
        assert_eq!(std::fs::read_link(&middle).expect("middle link"), real);
        assert_eq!(
            std::fs::symlink_metadata(account.join("config.toml"))
                .expect("link")
                .ino(),
            account_ino
        );
        assert_eq!(
            std::fs::symlink_metadata(&middle).expect("link").ino(),
            middle_ino
        );
    }

    #[test]
    fn a_link_replaced_before_exit_is_left_alone() {
        let mut fixture = spec_fixture();
        let shared = link_config(&mut fixture, SHARED_CONFIG);
        let link = PathBuf::from(fixture.shell_env["GROK_HOME"].clone()).join("config.toml");
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        std::fs::remove_file(&link).expect("unlink");
        std::fs::write(&link, "own = true\n").expect("plain file");
        let mut session = std::fs::read_to_string(session_config(&plan)).expect("copy");
        session = session.replace("theme = \"dark\"", "theme = \"tokyonight\"");
        std::fs::write(session_config(&plan), session).expect("session config");
        plan.shared_config_writeback.expect("writeback").publish();
        assert_eq!(
            std::fs::read_to_string(&link).expect("account file"),
            "own = true\n"
        );
        assert_eq!(
            std::fs::read_to_string(&shared).expect("shared"),
            SHARED_CONFIG
        );
    }

    #[test]
    fn a_section_that_is_not_a_table_is_left_unchanged() {
        let mut fixture = spec_fixture();
        let shared = link_config(&mut fixture, "ui = \"dark\"\n");
        let plan = plan_isolated(&fixture.spec()).expect("plan");
        std::fs::write(session_config(&plan), "[ui]\ntheme = \"tokyonight\"\n")
            .expect("session config");
        plan.shared_config_writeback.expect("writeback").publish();
        assert_eq!(
            std::fs::read_to_string(&shared).expect("shared"),
            "ui = \"dark\"\n"
        );
    }
}
