//! Grok Build adapter, the most intricate of the three: Grok has no flag for hooks at all.
//!
//! What does not work, and fails silently: `GROK_CONFIG` / `GROK_CONFIG_PATH` layer onto
//! `config.toml` for an allowlist of keys that drops `hooks` and `mcp_servers`;
//! `$GROK_HOME/managed_config.toml` and `requirements.toml` load but are deleted by the deployment
//! sync at runtime; `grok mcp add` only writes persistently to user or project scope.
//!
//! What works is a per-session `GROK_HOME` built as a symlink farm over the source home: the
//! console's pinned directory when set, else the shell's `GROK_HOME`, else `~/.grok`. Every entry is
//! a symlink back to the source's except the ones Octoboard has to own. `auth.json` and `sessions/`
//! stay symlinks, so login state is shared and a session Octoboard started stays resumable from the
//! user's own `grok` — plain for the default home, with `GROK_HOME=<that directory>` for a pinned
//! one.
//!
//! Two files in the farm are copies rather than symlinks, and both must stay that way:
//!
//! - `config.toml`, because Grok persists session settings back into it — `--reasoning-effort low`
//!   was observed rewriting `default_reasoning_effort` in the file. Against a symlink that write
//!   would either replace the link or be followed into the user's own configuration.
//! - `trusted_folders.toml`, for the same reason and one more: the project folder must be trusted
//!   or Grok silently loads none of the project's own `AGENTS.md`, hooks or MCP servers, and the
//!   trust store it consults is the one inside `GROK_HOME`. Octoboard adds the entry to its copy
//!   instead of making Grok write a trust decision into the user's store on their behalf.
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
//! the session persists lands in Octoboard's copy and is lost to them. Both are mitigated by
//! rebuilding the farm on every launch.
//!
//! Two conditions come from Grok itself rather than from the mechanism: it locates a project by
//! walking up for a `.git` directory and reads no project instructions *and* no project hooks
//! without one; and its hooks must be `command` type, because HTTP hooks cannot reach the daemon at
//! all — Grok's SSRF protection rejects both plain HTTP and private addresses. That same SSRF
//! protection is why the injected MCP server is a child process here too.
//!
//! The first of those conditions also decides where the role description comes from: a console's
//! working directory is not a repository, so a Grok console session reads no instruction file
//! written there and its whole role has to travel in `--rules`, which Grok persists into the
//! session record.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};
use crate::mcp;
use crate::protocol::{error_code, Agent, CodedError};

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

/// Entries Octoboard owns in its `GROK_HOME` rather than linking to the user's.
const COPIED_ENTRIES: &[&str] = &["config.toml", "trusted_folders.toml"];

/// Entries that must never exist in the farm: Grok's deployment sync deletes them at runtime, and
/// an organisation-distributed config is not Octoboard's to reproduce.
const EXCLUDED_ENTRIES: &[&str] = &["managed_config.toml", "requirements.toml", "hooks"];

pub struct GrokAdapter;

impl AgentAdapter for GrokAdapter {
    fn binary(&self) -> &'static str {
        "grok"
    }

    fn preallocates_session_id(&self) -> bool {
        true
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
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
        let grok_home = build_grok_home(spec, pinned)?;

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

        if spec.resume_agent_session_id.is_none() {
            if let Some(task) = spec.task {
                args.push(task.to_string());
            }
        }

        Ok(LaunchPlan {
            args,
            env: vec![(
                "GROK_HOME".to_string(),
                grok_home.to_string_lossy().into_owned(),
            )],
            ..LaunchPlan::default()
        })
    }
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

/// Builds the session's `GROK_HOME` and returns its path.
fn build_grok_home(spec: &LaunchSpec<'_>, pinned: Option<&Path>) -> Result<PathBuf> {
    let source_home = pinned
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_grok_home(spec));
    let farm = spec.scratch.join("grok-home");
    if farm.exists() {
        std::fs::remove_dir_all(&farm).ok();
    }
    std::fs::create_dir_all(&farm).with_context(|| format!("creating {}", farm.display()))?;

    if source_home.is_dir() {
        for entry in std::fs::read_dir(&source_home)
            .with_context(|| format!("reading {}", source_home.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if EXCLUDED_ENTRIES.contains(&name_str.as_ref())
                || COPIED_ENTRIES.contains(&name_str.as_ref())
            {
                continue;
            }
            let link = farm.join(&name);
            std::os::unix::fs::symlink(entry.path(), &link)
                .with_context(|| format!("linking {}", link.display()))?;
        }
    }

    let config = farm.join("config.toml");
    copy_if_present(&source_home.join("config.toml"), &config)?;
    append_mcp_server(&config, spec)?;

    write_trusted_folders(&source_home, &farm, spec.cwd)?;
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
    content.push_str(&format!(
        "\n[mcp_servers.{key}]\ncommand = {command}\nargs = [{args}]\n",
        key = mcp::SERVER_KEY,
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

fn copy_if_present(from: &Path, to: &Path) -> Result<()> {
    if from.is_file() {
        std::fs::copy(from, to).with_context(|| format!("copying {}", from.display()))?;
    }
    Ok(())
}

/// Copies the user's trust store and adds this project's directory to the copy, so the project's
/// own configuration loads without Grok ever writing a trust decision into the user's own file.
fn write_trusted_folders(source_home: &Path, farm: &Path, cwd: &Path) -> Result<()> {
    let source = source_home.join("trusted_folders.toml");
    let mut content = std::fs::read_to_string(&source).unwrap_or_default();
    let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let path_text = canonical.to_string_lossy();
    let header = format!("[folders.{}]", json!(path_text.as_ref()));
    if !content.contains(&header) {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!(
            "\n{header}\ntrusted = true\ndecided_at = {}\n",
            crate::protocol::now_millis() / 1000
        ));
    }
    std::fs::write(farm.join("trusted_folders.toml"), content)
        .with_context(|| format!("writing the trust store in {}", farm.display()))?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::{spec_fixture, SpecFixture};

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

        let plan = GrokAdapter.plan(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        // Still Octoboard's own farm that Grok runs against, never the chosen directory itself.
        assert_eq!(plan.env.len(), 1);
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
            pinned.join("auth.json")
        );
        assert_eq!(
            std::fs::read_link(farm.join("sessions")).expect("a link"),
            pinned.join("sessions")
        );
    }

    #[test]
    fn a_pinned_grok_home_that_has_gone_refuses_resuming_a_conversation() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some("agent-side-id");
        let err = GrokAdapter.plan(&spec).err().expect("refused");
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
        let err = GrokAdapter.plan(&fixture.spec()).err().expect("refused");
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
        let err = GrokAdapter.plan(&fixture.spec()).err().expect("refused");
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
        GrokAdapter.plan(&fixture.spec()).expect("not refused");
    }

    #[test]
    fn the_farm_links_the_users_home_but_owns_the_files_grok_writes_back_to() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        let plan = GrokAdapter.plan(&fixture.spec()).expect("plan");

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

    #[test]
    fn the_farms_trust_store_covers_the_project_without_touching_the_users() {
        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::write(
            home.join("trusted_folders.toml"),
            "[folders.\"/somewhere/else\"]\ntrusted = true\n",
        )
        .expect("trust store");

        let plan = GrokAdapter.plan(&fixture.spec()).expect("plan");
        let farm = PathBuf::from(&plan.env[0].1);
        let copy = std::fs::read_to_string(farm.join("trusted_folders.toml")).expect("the copy");
        let canonical = std::fs::canonicalize(&fixture.cwd).expect("canonical path");
        assert!(
            copy.contains("/somewhere/else"),
            "the user's entries survive"
        );
        assert!(
            copy.contains(canonical.to_string_lossy().as_ref()),
            "the project is trusted in the copy: without it Grok silently loads none of the \
             project's own configuration"
        );
        // The user's own store is untouched.
        let original =
            std::fs::read_to_string(home.join("trusted_folders.toml")).expect("the original");
        assert!(!original.contains(canonical.to_string_lossy().as_ref()));
    }

    #[test]
    fn the_mcp_server_is_appended_to_the_farms_config_without_losing_the_users_own() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let plan = GrokAdapter.plan(&fixture.spec()).expect("plan");
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

    /// Grok reads no project instruction file without a git root, so a console session's role can
    /// only come from `--rules` — and the flag is passed for every session, not just a console
    /// session's.
    #[test]
    fn the_role_text_travels_in_rules() {
        let mut fixture = spec_fixture();
        with_default_home(&mut fixture);
        let plan = GrokAdapter.plan(&fixture.spec()).expect("plan");
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
        let plan = GrokAdapter.plan(&fixture.spec()).expect("plan");
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
}
