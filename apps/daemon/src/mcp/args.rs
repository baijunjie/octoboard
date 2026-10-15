//! Reading a tool call's arguments, and finding the records they name within the caller's reach.
//!
//! A malformed boolean, string list or text argument is refused with a reason rather than read as
//! absent: a `"false"` that quietly changed nothing would look like success to the model that sent
//! it. And a project is
//! found only among the caller's own console's, so an id from another console resolves to nothing.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

use crate::protocol::{Agent, Console, Project, Session};
use crate::state::AppState;

pub(super) fn required_str<'a>(arguments: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("`{key}` is required"))
}

/// The strings of an optional array argument; anything in it that is not a string is skipped.
pub(super) fn string_list(arguments: &Map<String, Value>, key: &str) -> Vec<String> {
    arguments
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn optional_string(arguments: &Map<String, Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn optional_agent(arguments: &Map<String, Value>, key: &str) -> Result<Option<Agent>> {
    optional_string(arguments, key)
        .map(|value| parse_agent(&value))
        .transpose()
}

pub(super) fn parse_agent(value: &str) -> Result<Agent> {
    Agent::from_wire_name(value)
        .ok_or_else(|| anyhow!("`{value}` is not one of `claude`, `codex` or `grok`"))
}

/// A boolean argument. `None` when absent; anything but a boolean is refused rather than read as
/// absent, since a `"false"` that quietly changed nothing would look like success.
pub(super) fn optional_bool(arguments: &Map<String, Value>, key: &str) -> Result<Option<bool>> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => bail!("`{key}` must be true or false"),
    }
}

/// An array-of-strings argument. `None` when absent or `null`, so that a field which replaces a
/// list is never emptied by one that was not meant as a list; anything but an array of strings is
/// refused.
pub(super) fn optional_string_list(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("`{key}` must be an array of strings"))
            })
            .collect::<Result<Vec<_>>>()
            .map(Some),
        Some(_) => bail!("`{key}` must be an array of strings"),
    }
}

/// A string argument as given, blank included, for a field whose rule about blank is the
/// daemon's own. `None` when the argument is absent.
pub(super) fn raw_text(arguments: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    match arguments.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => bail!("`{key}` must be a string"),
    }
}

/// A string-or-null argument that keeps absent apart from cleared, as a setting that can be
/// left alone or put back to the agent's own default needs: `None` when absent, `Some(None)` for
/// `null` or a blank string.
pub(super) fn present_text(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>> {
    match arguments.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(text)) => Ok(Some(Some(text.clone()))),
        Some(_) => bail!("`{key}` must be a string or null"),
    }
}

pub(super) fn console_projects(state: &Arc<AppState>, console_id: &str) -> Result<Vec<Project>> {
    Ok(state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project.console_id == console_id)
        .collect())
}

/// Finds a project by id, or by name when that names exactly one. The console session works from
/// what `list_projects` told it, which is both, and a name it half-remembers must not resolve to
/// whichever project happens to sort first.
pub(super) fn resolve_project(
    state: &Arc<AppState>,
    console_id: &str,
    wanted: &str,
) -> Result<Project> {
    let projects = console_projects(state, console_id)?;
    if let Some(project) = projects.iter().find(|project| project.id == wanted) {
        return Ok(project.clone());
    }
    let matches: Vec<&Project> = projects
        .iter()
        .filter(|project| project.name == wanted)
        .collect();
    match matches.as_slice() {
        [project] => Ok((*project).clone()),
        [] => bail!("this console has no project `{wanted}`"),
        _ => bail!(
            "`{wanted}` names {} projects in this console; use the project's id instead",
            matches.len()
        ),
    }
}

/// The project an unbound project session starts sessions in: its own, always. A `project` it
/// names must be that one, by id or by name, so a call aimed at another project is refused rather
/// than quietly run in this one.
pub(super) fn own_project(
    state: &Arc<AppState>,
    caller: &Session,
    named: Option<String>,
) -> Result<Project> {
    let project = caller
        .project_id
        .as_deref()
        .map(|id| state.store.get_project(id))
        .transpose()?
        .flatten()
        .ok_or_else(|| anyhow!("this session's project is no longer on record"))?;
    if let Some(named) = named {
        if named != project.id && named != project.name {
            bail!(
                "you can start sessions only in your own project, `{}` (\"{}\"), not in `{named}`",
                project.id,
                project.name
            );
        }
    }
    Ok(project)
}

/// The console the caller belongs to; the only one a console session reads or changes.
pub(super) fn own_console(state: &Arc<AppState>, caller: &Session) -> Result<Console> {
    state
        .store
        .get_console(&caller.console_id)?
        .ok_or_else(|| anyhow!("this session's console is no longer on record"))
}
