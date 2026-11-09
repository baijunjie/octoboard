//! Grok Build as a consumer of compatibility sources: Codex's (the switch `codex_compatibility`)
//! and, where Grok's own reading of it falls short, Claude Code's (`claude_compatibility`). What a
//! source gives goes into the session's own copy of `config.toml`, edited as a document, never as
//! appended text: a second `[mcp_servers.<name>]` table or a second `[skills]` is a TOML error that
//! would make Grok drop its whole configuration. A copy that cannot be parsed is left as it is,
//! and the launch goes on without the source.
//!
//! - MCP servers become `[mcp_servers.<name>]` tables (`command`, `args`, `env`, or `url` and
//!   `headers`). Grok expands `${NAME}` in `args`, `env` values and `headers` values and starts a
//!   server with its own environment, so a literal value is not written to the file: it goes into
//!   the Grok process's environment as `OCTOBOARD_<SOURCE>_MCP_<n>_<m>`, `<n>` the server's
//!   position among its source's servers and `<m>` the entry's within its `env` or `headers`, and
//!   the table refers to it (`compatibility_offer::translate`). What stays in the file, which is
//!   owner-only like the rest of the copy, is the literal parts of a value that mixes text with a
//!   reference and the `default` of a `${NAME:-default}`, which Grok also expands (measured on
//!   Grok Build 1.0.77), so both forms of reference are kept; a value with any other `${` leaves
//!   its server out. A name Grok defines itself is never taken from a source, in the copy of the
//!   user's configuration or in a project's `.grok/config.toml` from the working directory up to
//!   the repository root, and with a project file that exists but cannot be read no server is
//!   taken at all. A name two sources both give goes to the first: Claude Code's, then Codex's,
//!   whether Claude Code's servers are given here or read by Grok itself.
//! - Codex's skills are linked, one directory each, into `codex-skills` of the session's scratch
//!   space, which is added to `[skills] paths`, after the paths already there, less the ones under
//!   an `.agents/skills`, which Grok reads itself. Grok loads `<path>/<name>/SKILL.md`. A
//!   `commands/` directory under such a path is not read as slash commands (`grok inspect` on
//!   Grok Build 1.0.77 lists none), so the source's custom prompts are left to its instructions
//!   (`plan` appends them to `--rules`).
//!
//! The copy is the session's own and what is added is never carried back to the user's file
//! (`writeback`); it is replaced whole, so Grok never reads it half written.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use toml_edit::{value, Array, DocumentMut, Item, Table};

use crate::adapter::account_cli::snapshot_home;
use crate::adapter::compatibility_offer::{is_env_name, is_name, offer_entries, translate};
use crate::adapter::files::{
    is_agent_config_dir, project_dirs, read_bounded_text, read_regular_text, LARGE_FILE_LIMIT,
    SMALL_FILE_LIMIT,
};
use crate::adapter::{CompatibilitySource, LaunchSpec};
use crate::protocol::AgentId;

/// The directory of the session's scratch space holding the links to Codex's skills.
const SKILLS_DIR: &str = "codex-skills";

/// The Grok process variable of its Claude Code MCP reading (`GROK_CLAUDE_MCPS_ENABLED`).
pub(super) const CLAUDE_MCPS_VAR: &str = "GROK_CLAUDE_MCPS_ENABLED";

/// Whether the switch is on and the global config file Claude Code would read for this launch
/// (`CLAUDE_CONFIG_DIR/.claude.json` when the shell exports one) is not the `$HOME/.claude.json`
/// Grok reads whatever the shell exports, so that Grok's own reading of Claude Code's servers has
/// to be replaced by Octoboard giving them.
fn claude_config_elsewhere(spec: &LaunchSpec<'_>) -> bool {
    if spec.compatibility_source(AgentId::CLAUDE).is_none() {
        return false;
    }
    let in_effect = crate::adapter::claude::settings::global_config_file(spec.shell_env, None);
    let read_by_grok = snapshot_home(spec.shell_env).join(".claude.json");
    let canonical = |path: &Path| crate::hostfs::canonical(path).unwrap_or(path.to_path_buf());
    in_effect != read_by_grok && canonical(&in_effect) != canonical(&read_by_grok)
}

/// One source's servers to add, with the prefix of the variables that carry its values.
struct Supply {
    source: &'static dyn CompatibilitySource,
    prefix: &'static str,
    /// Whether a reference to a variable the launch lacks is settled as Codex's agent would have
    /// (`compatibility_offer::translate`); Claude Code's own are left to Grok to expand.
    settle: bool,
}

/// What [`add_to_config`] did for a launch.
#[derive(Default)]
pub(super) struct Added {
    /// The variables the Grok process needs for the values the servers refer to.
    pub env: Vec<(String, String)>,
    /// Whether Claude Code's servers are in the copy, so that Grok's own reading of them
    /// (`GROK_CLAUDE_MCPS_ENABLED`) is to be turned off. False wherever the copy could not be
    /// written, so that Grok keeps reading them itself.
    pub claude_mcps_supplied: bool,
}

/// Adds the sources' configuration to the session's `config.toml` (`source_home` is the Grok home
/// the session's was copied from); nothing is added when it cannot be read, parsed or replaced.
pub(super) fn add_to_config(config: &Path, source_home: &Path, spec: &LaunchSpec<'_>) -> Added {
    let claude = spec.compatibility_source(AgentId::CLAUDE);
    let claude_supplied = claude_config_elsewhere(spec);
    let mut supplies = Vec::new();
    if let Some(source) = claude.filter(|_| claude_supplied) {
        supplies.push(Supply {
            source,
            prefix: "OCTOBOARD_CLAUDE_MCP_",
            settle: false,
        });
    }
    let codex = spec.compatibility_source(AgentId::CODEX);
    if let Some(source) = codex {
        supplies.push(Supply {
            source,
            prefix: "OCTOBOARD_CODEX_MCP_",
            settle: true,
        });
    }
    if supplies.is_empty() {
        return Added::default();
    }
    let text = match read_regular_text(config, LARGE_FILE_LIMIT) {
        Ok(text) => text,
        Err(err) => {
            tracing::warn!(path = %config.display(), %err, "could not read the Grok configuration copy; no compatibility source is added");
            return Added::default();
        }
    };
    let mut document = match text.parse::<DocumentMut>() {
        Ok(document) => document,
        Err(err) => {
            tracing::warn!(path = %config.display(), %err, "could not parse the Grok configuration copy; no compatibility source is added");
            return Added::default();
        }
    };

    let mut env = Vec::new();
    let mut changed = false;
    let mut supplied = false;
    if let Some(mut taken) = defined_server_names(&document, source_home, spec.cwd) {
        // Where Grok reads Claude Code's servers itself, they are as good as defined: the name
        // goes to Claude Code's, as it does where they are given here.
        if let Some(source) = claude.filter(|_| !claude_supplied) {
            taken.extend(source.mcp_servers(spec.shell_env, spec.cwd).into_keys());
        }
        changed |= add_servers(&mut document, taken, &supplies, spec, &mut env);
        supplied = claude_supplied;
    }
    if let Some(source) = codex {
        if let Some(dir) = skills_dir(source, spec) {
            changed |= add_skills_path(&mut document, &dir);
        }
    }
    if changed {
        let written = crate::hostfs::replace::replace_beside(
            config,
            document.to_string().as_bytes(),
            crate::hostfs::replace::Permissions::Private,
        );
        if let Err(err) = written {
            tracing::warn!(path = %config.display(), %err, "could not write the compatibility sources into the Grok configuration copy");
            return Added::default();
        }
    }
    Added {
        env,
        claude_mcps_supplied: supplied,
    }
}

/// The names of the MCP servers Grok defines for a session in `cwd`: in the copy of the user's
/// configuration (Octoboard's own among them) and in the `.grok/config.toml` of each directory
/// from the working directory up to the repository root. `None` when such a project file exists
/// and cannot be read or parsed, so that the names are not known. A `.grok` that is the Grok home
/// itself, as in a home directory opened as the project, is the copy already read.
fn defined_server_names(
    document: &DocumentMut,
    source_home: &Path,
    cwd: &Path,
) -> Option<BTreeSet<String>> {
    let mut names = server_names(document.as_table());
    let project = crate::adapter::files::canonical_project(cwd);
    for dir in project_dirs(&project) {
        let path = dir.join(".grok/config.toml");
        if is_agent_config_dir(&dir.join(".grok"), source_home) {
            continue;
        }
        let text = match read_bounded_text(&path, &project, SMALL_FILE_LIMIT) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "could not read a project Grok config; no compatibility server is added");
                return None;
            }
        };
        match text.parse::<DocumentMut>() {
            Ok(file) => names.extend(server_names(file.as_table())),
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "could not parse a project Grok config; no compatibility server is added");
                return None;
            }
        }
    }
    names.insert(crate::session_tools::catalogue::SERVER_KEY.to_string());
    Some(names)
}

fn server_names(table: &Table) -> BTreeSet<String> {
    table
        .get("mcp_servers")
        .and_then(Item::as_table_like)
        .into_iter()
        .flat_map(|servers| servers.iter().map(|(name, _)| name.to_string()))
        .collect()
}

/// A table of `document` under `key`, made when absent and turned from an inline table into a
/// regular one; `None` when the key holds anything else.
fn table_of<'a>(document: &'a mut DocumentMut, key: &str) -> Option<&'a mut Table> {
    let item = document.as_table_mut().entry(key).or_insert_with(|| {
        let mut table = Table::new();
        table.set_implicit(true);
        Item::Table(table)
    });
    if item.is_inline_table() {
        *item = match std::mem::take(item).into_table() {
            Ok(table) => Item::Table(table),
            Err(item) => item,
        };
    }
    item.as_table_mut()
}

/// Adds each supply's servers that are not `taken`, the names it adds becoming taken. All of a
/// supply's names, those it cannot express as well, are taken once it is through, so that a later
/// supply does not give a server of the same name in its place. Returns whether the document
/// changed.
fn add_servers(
    document: &mut DocumentMut,
    mut taken: BTreeSet<String>,
    supplies: &[Supply],
    spec: &LaunchSpec<'_>,
    env: &mut Vec<(String, String)>,
) -> bool {
    let Some(servers) = table_of(document, "mcp_servers") else {
        return false;
    };
    let mut added = false;
    for supply in supplies {
        let from = supply.source.mcp_servers(spec.shell_env, spec.cwd);
        let names: Vec<String> = from.keys().cloned().collect();
        for (index, (name, raw)) in from.into_iter().enumerate() {
            if !is_name(&name) || taken.contains(&name) {
                continue;
            }
            let mut carried = Vec::new();
            match translate(
                &raw,
                supply.prefix,
                index,
                supply.settle.then_some(spec.shell_env),
                &mut carried,
            )
            .and_then(|server| table(&server))
            {
                Some(table) => {
                    servers.insert(&name, Item::Table(table));
                    taken.insert(name);
                    env.extend(carried);
                    added = true;
                }
                None => {
                    tracing::warn!(server = %name, "skipping a compatibility source's MCP server for Grok");
                }
            }
        }
        taken.extend(names);
    }
    added
}

/// A server of the common format (as `translate` gives it) as a Grok `mcp_servers` table, a remote
/// one (`http` or `sse`) as `url` and `headers`; `None` where a value holds a reference of other
/// than the forms `${NAME}` and `${NAME:-default}`.
fn table(server: &Value) -> Option<Table> {
    let object = server.as_object()?;
    let mut table = Table::new();
    if matches!(
        object.get("type").and_then(Value::as_str),
        Some("http" | "sse")
    ) {
        table["url"] = value(object.get("url")?.as_str()?);
    } else {
        table["command"] = value(object.get("command")?.as_str()?);
        let args: Array = object
            .get("args")?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        table["args"] = value(args);
    }
    for key in ["env", "headers"] {
        let Some(entries) = object.get(key) else {
            continue;
        };
        let mut entry_table = Table::new();
        for (name, text) in entries.as_object()? {
            let text = text.as_str().filter(|text| plain_references(text))?;
            entry_table[name.as_str()] = value(text);
        }
        table[key] = Item::Table(entry_table);
    }
    Some(table)
}

/// Whether every `${` of `text` opens a reference Grok expands, `${NAME}` or `${NAME:-default}`.
fn plain_references(text: &str) -> bool {
    let mut rest = text;
    while let Some(at) = rest.find("${") {
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else {
            return false;
        };
        let reference = &after[..end];
        let name = reference
            .split_once(":-")
            .map_or(reference, |(name, _)| name);
        if !is_env_name(name) {
            return false;
        }
        rest = &after[end + 1..];
    }
    true
}

/// Builds the directory of the source's skills that Grok does not read itself in the session's
/// scratch space, afresh: a link to each skill directory, on every host. `None` while there is
/// nothing to offer.
fn skills_dir(source: &dyn CompatibilitySource, spec: &LaunchSpec<'_>) -> Option<PathBuf> {
    let dir = spec.scratch.join(SKILLS_DIR);
    if let Err(err) = crate::hostfs::remove_any(&dir) {
        tracing::warn!(path = %dir.display(), %err, "could not clear the compatibility skills directory");
        return None;
    }
    // Grok reads the `.agents/skills` roots (the project's and the user's) itself.
    let skills = source
        .skill_dirs(spec.shell_env, spec.cwd)
        .into_iter()
        .filter(|skill| {
            !skill
                .parent()
                .is_some_and(|root| root.ends_with(".agents/skills"))
        })
        .collect();
    (offer_entries("Grok", &dir, skills, true, true) > 0).then_some(dir)
}

/// Appends `dir` to `[skills] paths`, creating either; `false` where `[skills]` or `paths` is
/// something else, which is left alone.
fn add_skills_path(document: &mut DocumentMut, dir: &Path) -> bool {
    let Some(skills) = table_of(document, "skills") else {
        return false;
    };
    let paths = skills.entry("paths").or_insert_with(|| value(Array::new()));
    match paths.as_array_mut() {
        Some(paths) => {
            paths.push(dir.to_string_lossy().as_ref());
            true
        }
        None => false,
    }
}
