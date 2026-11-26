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
//! Where the host makes no file links (Windows without Developer Mode or elevation), directories are
//! still linked, since a directory alias needs none, and every other file is a copy, the
//! session's own, lost with it, except that lock and temporary files are left out, for Grok to
//! create, and so is `auth.json`: Grok is pointed at the user's own login with `GROK_AUTH_PATH`
//! instead, so a sign-in or a token refresh in the session lands in the user's own file, as it does
//! through a link, and the user's own `grok` never holds a refresh token the session has spent.
//! Nothing else needs writing back: a session's conversations are under the linked `sessions/`,
//! and the two files below that are copies on every host are carried back as they always are.
//! Which entry is linked, copied or left out is decided by `home::carried`.
//!
//! Two files in the farm are copies rather than symlinks, and both must stay that way:
//!
//! - `config.toml`. The session home keeps a real copy so the MCP server appended to it stays out of
//!   the source file. Grok rewrites the copy for session settings. When the source file is a symlink,
//!   the theme, the plugin switches, and a model chosen in the session are written to that link's
//!   canonical target after the process exits ([`writeback::SharedConfigWriteback`]), and the link
//!   itself is left in place. The model and reasoning effort passed only for this launch are not
//!   written there, and neither is the MCP block or what a compatibility source adds
//!   (`compatibility_consumer`: its MCP servers and its skills path, edited into the copy as a
//!   document).
//! - `trusted_folders.toml`: Grok saves it by replacing the file, so against a symlink its write
//!   would replace the link rather than reach the user's store, and no setting moves the store out
//!   of `GROK_HOME`. The copy carries every folder the user already trusts. For a project session
//!   Octoboard adds nothing to it: a folder Grok has not been told to trust shows Grok's own
//!   confirmation, which Octoboard presses under the user's permission (`crate::session::trust`),
//!   or the person answers in the terminal. The entry Grok then writes lands only in this copy, so
//!   once the confirmation is accepted, by Octoboard's press or in the terminal, it is carried
//!   over, unchanged, to the user's own store (`LaunchPlan::carried_trust`, `trust`), and a later
//!   launch is trusted from the start. For a console session the copy is given an entry for the
//!   console's directory (`trust::trust_for_this_launch`), so Grok does not ask, and nothing is
//!   carried: the entry goes with the session's home and the user's store never names a console.
//!   Until a folder is trusted Grok silently loads none of the project's own
//!   `AGENTS.md`, hooks or MCP servers. Folder trust, not a git repository, is what gates the
//!   instruction files: outside one Grok reads only the working directory's own.
//!
//! Four more entries are real copies too, `bundled/`, `bundles/`, `hooks-paths` and
//! `sandbox.toml`: Grok's startup sandbox check refuses to start when they are symlinks (it
//! documents that it write-protects them under its sandbox). They are copied on every launch, even
//! though a sandboxed launch gets no farm (see `SANDBOX_CHECKED_ENTRIES` for why). A source that is
//! itself a symlink becomes a real file or directory in the farm, which is what Grok checks. An entry the source lacks stays absent, for Grok to create. A bundle
//! update Grok downloads during a session lands in the copy and is lost; the next launch copies
//! whatever the user's own `grok` has fetched since.
//!
//! A launch that would run under Grok's sandbox gets no farm (`require_unsandboxed`) and starts
//! bare against the source home, as it would in a plain terminal, because the sandbox lets Grok
//! write only to its home, the working directory and temp directories, with links resolved: the
//! farm's `sessions/` and `auth.json` links lead back into the source home, so creating a session
//! would fail. The profile is read from the sources Grok
//! layers (see `sandbox_setting`); macOS managed preferences (`ai.x.grok`) are not read, a known
//! gap that leaves such a launch to fail inside Grok. The home's own `requirements.toml` and
//! `managed_config.toml` are not read either, because the farm excludes them and Grok never loads
//! them from it; the system directory's are, on a host where Grok has one.
//!
//! A session pins its source home at creation, because the session records a resume looks for
//! live in that home's `sessions/`; the farm is still what Grok runs against, so nothing changes
//! except which directory the links and copies come from.
//!
//! A pinned directory must already be a real Grok home, one Grok has been run against, for a farm
//! to be built over it: the farm only links entries that exist in the source, so for an empty
//! directory Grok would create `sessions/` and `auth.json` inside the throw-away farm and lose
//! them. A launch on one that is not starts bare against it instead, where Grok writes both into
//! the directory itself; a switch to one is refused (`refuse_unlaunchable_account`).
//!
//! The copies are a snapshot: an edit the user makes while a session runs is not seen, and anything
//! the session persists lands in Octoboard's copy. Two things are kept: a project's trust entry
//! above, once the confirmation is accepted, and, when the source `config.toml` is a symlink, the
//! settings [`writeback::SharedConfigWriteback`] copies to its target. The rest is lost with the
//! process.
//! Rebuilding the farm on every launch is what picks up an edit made in between.
//!
//! Two conditions come from Grok itself rather than from the mechanism: it locates a project by
//! walking up for a `.git` directory and reads no project hooks without one; and its hooks must be
//! `command` type, because HTTP hooks cannot reach the daemon at all — Grok's SSRF protection
//! rejects both plain HTTP and private addresses. That same SSRF protection is why the injected
//! MCP server is a child process here too.
//!
//! A console's working directory is not a repository, but Grok still reads the instruction file
//! written there once the folder is trusted, which a console session's launch does in the
//! session's own copy of the trust store. So a Grok console session gets the same short pointer
//! in `--rules` as the other agents, and the role itself from that file. Grok freezes both
//! `--rules` and the instruction files for a session, replaying them on a resume or a fork, so a
//! change never reaches a session already begun.

pub(crate) mod accounts;
mod adoption;
mod compatibility_consumer;
mod conversation;
mod feed;
mod home;
mod hooks;
mod interaction;
mod login;
mod mcp_inventory;
pub(crate) mod models;
mod plugin_paths;
#[cfg(test)]
pub(crate) use plugin_paths::after_exit_for_test as plugin_paths_after_exit_for_test;
mod query_home;
pub(crate) mod quota;
pub(crate) mod records;
mod refusal;
mod sandbox;
pub(crate) mod screen;
mod shared_config;
mod switch_continue;
pub(crate) mod trust;
mod writeback;

#[cfg(test)]
mod compatibility_tests;
#[cfg(test)]
mod degradation_tests;
#[cfg(test)]
mod home_tests;
#[cfg(test)]
mod interaction_tests;
#[cfg(test)]
mod sandbox_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod writeback_tests;

use std::path::Path;

use anyhow::Result;

use super::{AgentProvider, EarlyExit, FirstHook, LaunchPlan, LaunchSpec};
use crate::protocol::{AgentId, Code, CodedError, Notice, SessionKind, TerminalAppearance};
use crate::providers::{grok, ProviderDescriptor};
use crate::session::screen::ScreenReader;
use crate::session_tools;
use home::{build_grok_home, canonical_cwd, GrokHome};
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

    fn interactions(&self) -> Option<&'static dyn crate::session::interaction::InteractionReader> {
        Some(&interaction::GrokInteractions)
    }

    fn has_status_integration(&self) -> bool {
        true
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

    fn rebuild_launch(&self, launch: super::AdoptedLaunch<'_>) -> super::RebuiltLaunch {
        launch
            .adoption
            .map(|record| adoption::rebuild(record, &crate::paths::consoles_dir()))
            .unwrap_or_default()
    }

    fn refuse_unlaunchable_account(&self, pinned: Option<&Path>) -> Result<()> {
        match pinned {
            Some(dir) => require_initialized_grok_home(dir),
            None => Ok(()),
        }
    }

    /// Grok's own installer keeps everything under `~/.grok`; `grok update` updates it, and `grok
    /// update --check --json` says what the latest version is without changing anything. Grok is
    /// published through no package manager.
    fn cli_packaging(&self) -> &'static super::CliPackaging {
        &super::CliPackaging {
            native_layouts: &[&[".grok"]],
            native_markers: &[],
            native_update: &["update"],
            native_latest: Some(super::NativeLatest {
                args: &["update", "--check", "--json"],
                version_field: "latestVersion",
            }),
            native_manifest: None,
            npm_packages: &[],
            homebrew_formula: None,
            homebrew_cask: None,
        }
    }

    /// Read from the name of the release download the binary is, without spawning a CLI process:
    /// through the launcher link the installer makes on macOS and Linux, or, where the launcher is a
    /// copy (Windows), the versioned download of the same size beside it (see
    /// [`copied_release_version`]).
    fn installed_version(&self, binary: &str) -> Option<super::AgentVersion> {
        let resolved = crate::hostfs::canonical(binary).ok()?;
        release_version(resolved.file_name()?.to_str()?)
            .or_else(|| copied_release_version(&resolved))
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

    /// `--resume <id>` for a resume, `--model`, and a new conversation's task after a bare `--`,
    /// run against the account's own home (the pinned one exported by the launch) rather than a
    /// per-session one.
    fn bare_plan(&self, spec: &LaunchSpec<'_>) -> super::BarePlan {
        let mut args = Vec::new();
        if let Some(id) = spec.resume_agent_session_id {
            args.push("--resume".to_string());
            args.push(id.to_string());
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        args.extend(task_args(spec));
        super::BarePlan {
            args,
            ..super::BarePlan::default()
        }
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        plan_under(spec, system_config_dir().as_deref())
    }

    /// The enhanced launch without `--no-leader`, where Grok refused exactly that option as it
    /// started: a release that drops the flag `grok --help` never listed. Any other early exit
    /// gives nothing, and the launch goes bare: a refusal of another option names something the
    /// session cannot do without (its conversation id, its role), and an exit that refused nothing
    /// says nothing an option left out would change.
    fn reduced_plan(&self, spec: &LaunchSpec<'_>, exit: &EarlyExit) -> Option<LaunchPlan> {
        reduced_plan_under(spec, exit, system_config_dir().as_deref())
    }

    /// Grok fires `SessionStart` as it starts, 0.5–1.0 s after the launch on a fresh conversation
    /// and on a resume alike, before anything is written into it (measured on 1.0.67), and every
    /// plan, the reduced one included, carries Octoboard's hooks in the per-session home.
    fn first_hook_due(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> FirstHook {
        FirstHook::AtStart
    }
}

/// The directory of Grok's fleet-wide `managed_config.toml` and `requirements.toml`: `/etc/grok`,
/// and none on Windows. The ones in its home are excluded from the farm, so Grok running against
/// it never loads them.
fn system_config_dir() -> Option<std::path::PathBuf> {
    crate::platform::policy::system_config_dir("grok")
}

/// The flag that keeps Grok's agent in the TUI's own process, whatever its configuration says of
/// leader mode (see `plan_with`).
const NO_LEADER: &str = "--no-leader";

/// Whether a plan passes [`NO_LEADER`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leader {
    Overridden,
    LeftToConfiguration,
}

/// `reduced_plan` with the directory of Grok's system-wide configuration passed in, as for
/// [`plan_under`].
fn reduced_plan_under(
    spec: &LaunchSpec<'_>,
    exit: &EarlyExit,
    system_dir: Option<&Path>,
) -> Option<LaunchPlan> {
    if refusal::refused_option(&exit.output)? != NO_LEADER {
        return None;
    }
    let mut plan = plan_with(spec, system_dir, Leader::LeftToConfiguration).ok()?;
    plan.notice = Some(leader_not_overridden());
    Some(plan)
}

/// The standing notice of a launch that could not pass [`NO_LEADER`].
fn leader_not_overridden() -> Notice {
    Notice::new(
        Code::of_provider(grok::LEADER_NOT_OVERRIDDEN),
        format!(
            "This version of Grok does not take `{NO_LEADER}`, so where its configuration turns \
             leader mode on, stopping this session cuts off a running turn without Grok's own \
             shutdown"
        ),
        &[],
    )
    // It holds for the whole launch.
    .standing()
}

/// `plan` with the directory of Grok's system-wide configuration passed in, `None` on a host
/// where Grok has none, so a test never reads the machine's own.
fn plan_under(spec: &LaunchSpec<'_>, system_dir: Option<&Path>) -> Result<LaunchPlan> {
    plan_with(spec, system_dir, Leader::Overridden)
}

/// `plan_under`, passing [`NO_LEADER`] or leaving leader mode to Grok's configuration.
fn plan_with(
    spec: &LaunchSpec<'_>,
    system_dir: Option<&Path>,
    leader: Leader,
) -> Result<LaunchPlan> {
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
    let GrokHome {
        dir: grok_home,
        env: home_env,
        claude_mcps_supplied,
    } = build_grok_home(spec, &source_home)?;

    // Grok's leader mode (`[cli] use_leader = true` in the user's configuration, which the farm
    // copies) makes this TUI only a client of a leader process it starts in a process group of
    // its own but in the TUI's session. The TUI exits at once on the stop's `SIGTERM` without
    // ending the turn, so the leader is reached only by the sweep of the exited TUI's session, a
    // `SIGKILL` mid-turn that fires no `shutdown` hooks and skips the session flush of Grok's own
    // shutdown.
    //
    // `--no-leader`, a TUI flag `grok --help` does not list, keeps the agent in this process,
    // which the `SIGTERM` ends gracefully, whatever the configuration says. A release that
    // refuses it as it starts is launched again without it (`reduced_plan`).
    let mut args = Vec::new();
    if leader == Leader::Overridden {
        args.push(NO_LEADER.to_string());
    }
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
    let mut rules = session_tools::role::role_description(
        spec.kind,
        spec.owner.as_ref(),
        AgentId::GROK,
        &spec.role_options(),
    );
    // The guidance of the Codex compatibility source, after the role text.
    if let Some(source) = spec.compatibility_source(AgentId::CODEX) {
        rules.push_str("\n\n");
        rules.push_str(&source.instructions(AgentId::GROK.label(), spec.shell_env));
    }
    args.push(rules);

    // Grok applies explicit permission modes to fresh sessions and resumes. Unset, its own
    // configuration and resume rules decide; the account's copied configuration stays intact.
    let permission_mode = match spec.settings.choice("approval_mode") {
        Some("ask") => Some("default"),
        Some("auto") => Some("auto"),
        Some("always_approve") => Some("bypassPermissions"),
        _ => None,
    };
    if let Some(mode) = permission_mode {
        args.extend(["--permission-mode".to_string(), mode.to_string()]);
    }

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

    // Everything after the `--` is the prompt, so it comes last.
    args.extend(task_args(spec));

    let session_store = grok_home.join(grok::GROK_TRUST_STORE);
    let folder = canonical_cwd(spec.cwd);
    // A console's directory is Octoboard's own, so it is trusted without asking, and nothing
    // about it may reach the user's trust store: no entry is carried for a console session. What
    // the provider declares (`Capabilities::console_trust`) is done here: the directory is trusted
    // in the session's copy of the store, for this launch alone. Where that fails Grok asks, and
    // the confirmation is answered as any console's is (`crate::session::trust`). A project's
    // trust is the user's decision, so the entry Grok writes for it is carried to their store.
    let carried_trust = if spec.kind == SessionKind::Console {
        let declared = AgentId::GROK.descriptor().capabilities.console_trust;
        if declared == crate::providers::ConsoleTrust::PerLaunch {
            if let Err(err) = trust::trust_for_this_launch(&session_store, &folder) {
                tracing::warn!(
                    session = %spec.session_id,
                    %err,
                    "the console's directory could not be trusted for this launch"
                );
            }
        }
        None
    } else {
        Some(trust::CarriedTrust {
            session_store: session_store.clone(),
            user_store: source_home.join(grok::GROK_TRUST_STORE),
            folder: folder.clone(),
            record_wait: trust::RECORD_TIMEOUT,
        })
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
    let (home_var, home_value) = grok::LAYOUT
        .config_dir_export(&grok_home)
        .expect("Grok Build's variable names its configuration directory itself");
    let mut env = vec![
        (
            home_var.to_string(),
            home_value.to_string_lossy().into_owned(),
        ),
        ("GROK_THEME".to_string(), theme.to_string()),
    ];
    env.extend(home_env);
    // Grok's own Claude compatibility is on unless its configuration or the user's shell turns it
    // off; these outrank both. Turned off in Octoboard, nothing is passed and Grok follows its own
    // configuration. Grok reads Claude Code's MCP servers from `$HOME/.claude.json` and ignores
    // `CLAUDE_CONFIG_DIR`, so where the user's Claude Code reads another file Grok's own reading
    // is turned off and the servers are given from the right one instead
    // (`compatibility_consumer`).
    if spec.settings.switch("claude_compatibility") {
        env.extend(CLAUDE_COMPATIBILITY_VARS.map(|var| {
            let on = !(claude_mcps_supplied && var == compatibility_consumer::CLAUDE_MCPS_VAR);
            (var.to_string(), on.to_string())
        }));
    }
    // Grok's memory off for this process: neither its `<memory>` instructions nor anything under
    // the home's `memory-v2/` (1.0.89, measured). Off by default, but `config.toml` or a managed
    // remote setting can turn it on; this outranks both, and only a pin in `requirements.toml`
    // outranks it. The documented variable rather than the `--no-memory` flag `--help` leaves out.
    if spec.native_memory_off {
        env.push((MEMORY_OFF.0.to_string(), MEMORY_OFF.1.to_string()));
    }
    let writeback = shared_config_writeback(&source_home, &grok_home, spec.model);
    // What an earlier process recorded in the user's plugin registry under a per-session home is
    // put right before this one starts, and this one's own is left for after its exit.
    let plugin_paths = plugin_paths::plugin_paths(&source_home, &grok_home);
    if let Some(plugin_paths) = &plugin_paths {
        plugin_paths.settle_earlier();
    }
    let adoption = match &carried_trust {
        Some(carried_trust) => {
            adoption::record(carried_trust, writeback.as_ref(), plugin_paths.as_ref())
        }
        None => adoption::record_without_carrier(
            &session_store,
            &folder,
            writeback.as_ref(),
            plugin_paths.as_ref(),
        ),
    };
    Ok(LaunchPlan {
        args,
        env,
        carried_trust: carried_trust.map(|carried| {
            std::sync::Arc::new(carried) as std::sync::Arc<dyn crate::session::trust::TrustCarrier>
        }),
        after_exit: adoption::after_exit(writeback, plugin_paths),
        adoption,
        ..LaunchPlan::default()
    })
}

/// The platforms Grok Build publishes native releases for, with the suffix their downloads carry:
/// a release is `grok-<version>-<platform><suffix>`, the name of its artifact on x.ai's server
/// (`https://x.ai/cli/install.sh` and `install.ps1`) and of the download Grok's updater keeps in
/// `~/.grok/downloads` (seen on macOS).
const RELEASE_PLATFORMS: [(&str, &str); 6] = [
    ("macos-aarch64", ""),
    ("macos-x86_64", ""),
    ("linux-aarch64", ""),
    ("linux-x86_64", ""),
    ("windows-x86_64", ".exe"),
    ("windows-aarch64", ".exe"),
];

/// The version in a release download's file name; `None` for any other name.
fn release_version(filename: &str) -> Option<super::AgentVersion> {
    let release = filename.strip_prefix("grok-")?;
    let (version, platform) = release.split_once('-')?;
    if !RELEASE_PLATFORMS
        .iter()
        .any(|(name, suffix)| platform.strip_suffix(suffix) == Some(name))
    {
        return None;
    }
    super::parse_version(version)
}

/// The version of a launcher that is a copy rather than a link: on Windows the installer copies
/// the download into `<grok dir>\bin\grok.exe` (`install.ps1`), and the name it downloads to,
/// `grok-windows-<arch>.exe`, carries no version, nor does the program itself (1.0.77 has no
/// version resource). A versioned release download in `<grok dir>\downloads` of exactly the launcher's size
/// is the release it was copied from; `None` when no one version matches, which leaves the version
/// to the caller's probe.
fn copied_release_version(launcher: &Path) -> Option<super::AgentVersion> {
    let bin = launcher.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let size = std::fs::metadata(launcher).ok()?.len();
    let mut found = None;
    for entry in std::fs::read_dir(bin.parent()?.join("downloads")).ok()? {
        let Ok(entry) = entry else { continue };
        let Some(version) = entry.file_name().to_str().and_then(release_version) else {
            continue;
        };
        if !entry
            .metadata()
            .is_ok_and(|meta| meta.is_file() && meta.len() == size)
        {
            continue;
        }
        match found {
            None => found = Some(version),
            Some(other) if other == version => {}
            Some(_) => return None,
        }
    }
    found
}

/// The environment entry that turns Grok's memory off for one launch.
const MEMORY_OFF: (&str, &str) = ("GROK_MEMORY", "0");

/// The environment variables of Grok's `compat.claude.{skills,rules,agents,mcps,hooks}`
/// configuration, which outrank the configuration files.
const CLAUDE_COMPATIBILITY_VARS: [&str; 5] = [
    "GROK_CLAUDE_SKILLS_ENABLED",
    "GROK_CLAUDE_RULES_ENABLED",
    "GROK_CLAUDE_AGENTS_ENABLED",
    compatibility_consumer::CLAUDE_MCPS_VAR,
    "GROK_CLAUDE_HOOKS_ENABLED",
];

/// Fails a pinned source home Grok has never been run against: it carries no `sessions/`
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

/// A new conversation's task, the plan and the bare plan alike: after a bare `--`, which ends
/// Grok's options, since a task that starts with `-` is otherwise refused as an unexpected
/// argument, or read as an option (`-p x` runs a headless turn). A resume is given no task.
fn task_args(spec: &LaunchSpec<'_>) -> Vec<String> {
    spec.task
        .filter(|_| spec.resume_agent_session_id.is_none())
        .map(|task| vec!["--".to_string(), task.to_string()])
        .unwrap_or_default()
}
