//! Grok Build adapter, the most intricate of the three: Grok has no flag for hooks at all.
//!
//! What does not work, and fails silently: `GROK_CONFIG` / `GROK_CONFIG_PATH` layer onto
//! `config.toml` for an allowlist of keys that drops `hooks` and `mcp_servers`;
//! `$GROK_HOME/managed_config.toml` and `requirements.toml` load but are deleted by the deployment
//! sync at runtime; `grok mcp add` only writes persistently to user or project scope.
//!
//! What works is a per-session `GROK_HOME` built as a symlink farm over the user's real `~/.grok`:
//! every entry is a symlink back to theirs except the ones Octoboard has to own. `auth.json` and
//! `sessions/` stay symlinks, so login state is shared and a session Octoboard started stays
//! resumable from the user's own plain `grok`.
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
//! The copies are a snapshot: an edit the user makes while a session runs is not seen, and anything
//! the session persists lands in Octoboard's copy and is lost to them. Both are mitigated by
//! rebuilding the farm on every launch.
//!
//! Two conditions come from Grok itself rather than from the mechanism: it locates a project by
//! walking up for a `.git` directory and reads no project instructions *and* no project hooks
//! without one; and its hooks must be `command` type, because HTTP hooks cannot reach the daemon at
//! all — Grok's SSRF protection rejects both plain HTTP and private addresses.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use super::{AgentAdapter, LaunchPlan, LaunchSpec, HOOK_TIMEOUT_SECS};

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
        let grok_home = build_grok_home(spec)?;

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
        })
    }
}

/// Builds the session's `GROK_HOME` and returns its path.
fn build_grok_home(spec: &LaunchSpec<'_>) -> Result<PathBuf> {
    let real_home = real_grok_home(spec);
    let farm = spec.scratch.join("grok-home");
    if farm.exists() {
        std::fs::remove_dir_all(&farm).ok();
    }
    std::fs::create_dir_all(&farm).with_context(|| format!("creating {}", farm.display()))?;

    if real_home.is_dir() {
        for entry in std::fs::read_dir(&real_home)
            .with_context(|| format!("reading {}", real_home.display()))?
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

    copy_if_present(&real_home.join("config.toml"), &farm.join("config.toml"))?;
    // TODO(milestone 02): append the `[mcp_servers.octoboard]` block to this copy.

    write_trusted_folders(&real_home, &farm, spec.cwd)?;
    write_hooks(&farm, spec.hook_script)?;

    Ok(farm)
}

/// The user's own Grok home, read from the launch environment rather than the daemon's: `GROK_HOME`
/// is a user setting, and the daemon's own copy of it may belong to another Octoboard session's
/// farm when the daemon was started from inside one.
fn real_grok_home(spec: &LaunchSpec<'_>) -> PathBuf {
    if let Some(home) = spec
        .shell_env
        .get("GROK_HOME")
        .filter(|home| !home.is_empty())
    {
        return PathBuf::from(home);
    }
    // Also the user's own `HOME` rather than the daemon's, for the same reason.
    match spec.shell_env.get("HOME").filter(|home| !home.is_empty()) {
        Some(home) => PathBuf::from(home).join(".grok"),
        None => crate::paths::home_dir().join(".grok"),
    }
}

fn copy_if_present(from: &Path, to: &Path) -> Result<()> {
    if from.is_file() {
        std::fs::copy(from, to).with_context(|| format!("copying {}", from.display()))?;
    }
    Ok(())
}

/// Copies the user's trust store and adds this project's directory to the copy, so the project's
/// own configuration loads without Grok ever writing a trust decision into the user's own file.
fn write_trusted_folders(real_home: &Path, farm: &Path, cwd: &Path) -> Result<()> {
    let source = real_home.join("trusted_folders.toml");
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
    fn with_real_home(fixture: &mut SpecFixture) -> PathBuf {
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
    fn the_farm_links_the_users_home_but_owns_the_files_grok_writes_back_to() {
        let mut fixture = spec_fixture();
        let home = with_real_home(&mut fixture);
        let plan = GrokAdapter.plan(&fixture.spec()).expect("plan");

        let farm = PathBuf::from(&plan.env[0].1);
        assert_eq!(plan.env[0].0, "GROK_HOME");

        // Login state and session records stay shared with the user's own `grok`.
        assert!(farm.join("auth.json").is_symlink());
        assert!(farm.join("sessions").is_symlink());
        // Grok persists session settings into `config.toml`, so against a symlink that write would
        // reach the user's own file.
        assert!(!farm.join("config.toml").is_symlink());
        assert_eq!(
            std::fs::read_to_string(farm.join("config.toml")).expect("the copy"),
            std::fs::read_to_string(home.join("config.toml")).expect("the original")
        );
        // The deployment sync deletes these at runtime, so the farm must not carry them.
        assert!(!farm.join("managed_config.toml").exists());
        assert!(!farm.join("requirements.toml").exists());
    }

    #[test]
    fn the_farms_trust_store_covers_the_project_without_touching_the_users() {
        let mut fixture = spec_fixture();
        let home = with_real_home(&mut fixture);
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
    fn the_hooks_are_command_type_with_an_explicit_timeout() {
        let mut fixture = spec_fixture();
        with_real_home(&mut fixture);
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
