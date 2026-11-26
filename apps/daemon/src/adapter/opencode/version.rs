use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use rusqlite::{Connection, OpenFlags};

use crate::adapter::{account_cli, account_dir, version_from_output, AgentProvider, AgentVersion};
use crate::protocol::{Code, CodedError};
use crate::providers::opencode::MIGRATION_REQUIRED;
use crate::providers::AgentId;

use super::{LaunchSpec, OpenCodeProvider};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Generation {
    V1,
    V2,
}

impl Generation {
    pub(super) fn plugin_key(self) -> &'static str {
        match self {
            Self::V1 => "plugin",
            Self::V2 => "plugins",
        }
    }
}

pub(super) fn installed(env: &HashMap<String, String>) -> Result<Option<AgentVersion>> {
    let binary = account_cli::resolve_account_binary(AgentId::OPENCODE, env)
        .map_err(|err| err.into_error())?;
    Ok(OpenCodeProvider
        .installed_version(&binary.program.to_string_lossy())
        .or_else(|| {
            let output = account_cli::query(
                AgentId::OPENCODE,
                env,
                None,
                &["--version"],
                Duration::from_secs(3),
                4096,
                None,
            )
            .ok()?;
            output.status.success().then_some(())?;
            version_from_output(std::str::from_utf8(&output.stdout).ok()?)
        }))
}

/// The first version of OpenCode's second generation: its launch (`--standalone`, `--server`), its
/// server's `/api/…` routes, its configuration keys (`plugins`, `permissions`) and its records
/// (`docs/agent-cli-reference.md`, "OpenCode 2"). The one floor OpenCode's launch keeps
/// (`docs/plans/terminal-first-degradation.md`, D12), known from the releases themselves: 1.19.6,
/// the last of the `opencode-ai` package, offers `/update to upgrade to OpenCode 2.0` on its
/// start screen, the second generation is published from 2.0.0 on as `@opencode/cli`, and every
/// 2.x release run (2.3.2, 2.4.0, 2.4.1) has the second generation's formats. A version below it
/// launches as the first generation, any other as the second, the newest. A wrong pick that makes
/// the CLI exit as it starts has the launch started again bare.
const SECOND_GENERATION: AgentVersion = (2, 0, 0);

/// The generation a launch of `version` is planned for: the second from [`SECOND_GENERATION`] on,
/// and for a version that could not be read, which is taken for the newest.
pub(super) fn generation_of(version: Option<AgentVersion>) -> Generation {
    match version {
        Some(version) if version < SECOND_GENERATION => Generation::V1,
        _ => Generation::V2,
    }
}

/// The generation of the launch `spec` plans: of the version the launch read, else of the one
/// installed. A version that cannot be asked for is taken for the newest, like one that cannot be
/// read.
pub(super) fn at_launch(spec: &LaunchSpec<'_>) -> Generation {
    let version = match spec.agent_version {
        Some(version) => Some(version),
        None => installed(spec.shell_env).ok().flatten(),
    };
    generation_of(version)
}

/// The cause of an enhanced launch of the second generation that cannot be prepared while a
/// migration of OpenCode's own is pending (`provider.opencode.migration_required`): Octoboard's
/// commands would run before it, so the launch is bare, and OpenCode migrates as it starts.
pub(super) fn refuse_pending_migration(
    spec: &LaunchSpec<'_>,
    generation: Generation,
) -> Result<()> {
    if generation == Generation::V2 && migration_pending(spec.shell_env) {
        return Err(CodedError::raised(
            Code::of_provider(MIGRATION_REQUIRED),
            "OpenCode has yet to migrate its own configuration to its new version",
            &[],
        ));
    }
    Ok(())
}

pub(super) fn database(env: &HashMap<String, String>) -> PathBuf {
    let data = account_dir(AgentId::OPENCODE, None, env);
    data.join(env.get("OPENCODE_DB").map_or("opencode.db", String::as_str))
}

fn immutable_database(path: &Path) -> rusqlite::Result<Connection> {
    let path = crate::platform::names::file_uri_path(path)
        .replace('%', "%25")
        .replace('?', "%3F")
        .replace('#', "%23");
    // An immutable read prevents opening or creating SQLite's WAL sidecars. A migration visible
    // only in the current WAL is picked up after the user's OpenCode closes.
    // no-file-write: an immutable read of the migration marker only.
    Connection::open_with_flags(
        format!("file:{path}?immutable=1"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
}

fn migrated_credentials(path: &Path) -> bool {
    immutable_database(path)
        .and_then(|db| {
            db.query_row(
                "SELECT EXISTS(SELECT 1 FROM migration WHERE id = ?1)",
                ["20260805200742_import_legacy_credentials"],
                |row| row.get(0),
            )
        })
        .unwrap_or(false)
}

// A v2 server can begin answering while it is still importing v1 conversations.
pub(super) fn legacy_records_pending(env: &HashMap<String, String>) -> bool {
    let Ok(db) = immutable_database(&database(env)) else {
        return false;
    };
    let legacy: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'session')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !legacy {
        return false;
    }
    let phase: Option<String> = db
        .query_row(
            "SELECT json_extract(value, '$.phase') FROM kv WHERE key = 'migration.v1-v2'",
            [],
            |row| row.get(0),
        )
        .ok();
    phase.as_deref() != Some("completed")
}

fn xdg_dir(env: &HashMap<String, String>, var: &str, fallback: &str) -> Option<PathBuf> {
    env.get(var)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env.get("HOME").map(|home| Path::new(home).join(fallback)))
        .map(|dir| dir.join("opencode"))
}

pub(super) fn migration_pending(env: &HashMap<String, String>) -> bool {
    let data = account_dir(AgentId::OPENCODE, None, env);
    if data.join("auth.json").exists() && !migrated_credentials(&database(env)) {
        return true;
    }
    xdg_dir(env, "XDG_CONFIG_HOME", ".config").is_some_and(|config| {
        let cli = config.join("cli.json");
        if !cli.exists() {
            let tui = config.join("tui.json");
            let saved_tui_preferences = [
                "theme",
                "theme_mode_lock",
                "attention_sound_pack",
                "diff_viewer_view",
                "thinking_mode",
                "thinking_visibility",
                "diff_wrap_mode",
                "diff_viewer_show_file_tree",
                "diff_viewer_single_patch",
                "terminal_title_enabled",
                "file_context_enabled",
                "paste_summary_enabled",
                "sidebar",
                "scrollbar_visible",
                "exploration_grouping",
                "animations_enabled",
            ];
            let legacy_keys = [
                "theme",
                "keybinds",
                "plugin",
                "plugin_enabled",
                "leader_timeout",
                "scroll_speed",
                "scroll_acceleration",
                "attention",
                "diff_style",
                "mouse",
                "cursor",
            ];
            return has_settings(&tui, &legacy_keys)
                || xdg_dir(env, "XDG_STATE_HOME", ".local/state").is_some_and(|state| {
                    has_settings(&state.join("kv.json"), &saved_tui_preferences)
                });
        }
        read_settings(&cli).is_some_and(|config| {
            config["terminal"].get("copy_on_select").is_some()
                || config["keybinds"]
                    .as_object()
                    .is_some_and(|keys| keys.keys().any(|key| !key.contains('.')))
        })
    })
}

fn read_settings(path: &Path) -> Option<serde_json::Value> {
    let text = super::read_regular_text(path, super::SMALL_FILE_LIMIT).ok()?;
    super::user_json::parse(&text, super::CONFIG_TAKES).ok()
}

fn has_settings(path: &Path, keys: &[&str]) -> bool {
    read_settings(path).is_some_and(|config| keys.iter().any(|key| config.get(*key).is_some()))
}

// OpenCode 2 inspection commands otherwise connect to or start its persistent service.
pub(super) fn background_v2(env: &HashMap<String, String>) -> Option<bool> {
    match installed(env) {
        Ok(Some(version)) if version >= SECOND_GENERATION => {
            (!migration_pending(env)).then_some(true)
        }
        Ok(None) if migration_pending(env) => None,
        _ => Some(false),
    }
}
