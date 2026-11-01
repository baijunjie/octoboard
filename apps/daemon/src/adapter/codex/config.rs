//! Reading the Codex home in effect: its `config.toml`, the MCP server names and the model
//! migration table.

use std::collections::BTreeSet;
use std::path::Path;

use crate::adapter::files::{
    is_agent_config_dir, read_bounded_text, read_regular_text, SMALL_FILE_LIMIT,
};
use crate::agents::layout;
use crate::records::line_file::JSON_FILE_LIMIT;

/// The `notice.model_migrations` table for the models cache in `codex_home`: every model that has
/// an upgrade target, mapped to it, so that whichever model ends up in effect, Codex's "try the new
/// model" prompt is not shown. `None` when the cache is missing, unreadable within its size bound
/// or lists no upgrade. Read from the file, since asking Codex would start a process on every
/// launch.
pub(super) fn model_migrations(codex_home: &Path) -> Option<String> {
    let text = read_regular_text(
        &codex_home.join(layout::CODEX_MODELS_CACHE),
        JSON_FILE_LIMIT as usize,
    )
    .ok()?;
    upgrade_table(&text)
}

pub(super) fn upgrade_table(cache: &str) -> Option<String> {
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
                .then(|| format!("{}={}", super::toml_string(from), super::toml_string(to)))
        })
        .collect();
    (!entries.is_empty()).then(|| format!("{{{}}}", entries.join(",")))
}

/// What the user's `config.toml` in the Codex home in effect came to, for each reader to take its
/// own safe side of.
pub(super) enum ConfigFile {
    Absent,
    /// It exists but cannot be read within its size bound, or parsed.
    Unreadable,
    Parsed(toml::Table),
}

pub(super) fn read_config(codex_home: &Path) -> ConfigFile {
    let text = match read_regular_text(
        &codex_home.join(layout::CODEX_CONFIG_FILE),
        SMALL_FILE_LIMIT,
    ) {
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
pub(super) fn server_names(config: &toml::Table) -> BTreeSet<String> {
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
pub(super) fn project_server_names(project: &Path, codex_home: &Path) -> Option<BTreeSet<String>> {
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
