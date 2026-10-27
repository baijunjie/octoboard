//! Reading a tool call's arguments, and finding the records they name within the caller's reach.
//!
//! A malformed boolean, string list or text argument is refused with a reason rather than read as
//! absent: a `"false"` that quietly changed nothing would look like success to the model that sent
//! it. A project named for a write (`resolve_project`) is found only among the caller's own
//! console's, so an id from another console resolves to nothing; the reads (`find_project`) take
//! an id from any console, and a name from the caller's console first, else from all.

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

/// A whole number argument from `min` to `max`. `None` when absent or `null`; anything else, a
/// fraction or a negative number included, is refused rather than read as absent.
pub(super) fn optional_count(
    arguments: &Map<String, Value>,
    key: &str,
    min: usize,
    max: usize,
) -> Result<Option<usize>> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .filter(|count| (min..=max).contains(count))
            .map(Some)
            .ok_or_else(|| {
                if max == usize::MAX {
                    anyhow!("`{key}` must be a whole number, {min} or more")
                } else {
                    anyhow!("`{key}` must be a whole number from {min} to {max}")
                }
            }),
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

/// An agent-or-null argument that keeps absent apart from cleared, like [`present_text`]: `None`
/// when absent, `Some(None)` for `null`.
pub(super) fn present_agent(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Option<Option<Agent>>> {
    match arguments.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(text)) => Ok(Some(Some(parse_agent(text)?))),
        Some(_) => bail!("`{key}` must be an agent or null"),
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

/// The one of `named`, which are the records whose name is `wanted`, or `None` for none; more than
/// one is refused, because a name half-remembered must not resolve to whichever record happens to
/// sort first. `kind` and `scope` word the refusal: "projects" and " in this console".
fn unique<'a, T>(
    named: impl Iterator<Item = &'a T>,
    wanted: &str,
    kind: &str,
    scope: &str,
) -> Result<Option<&'a T>> {
    let named: Vec<&T> = named.collect();
    match named.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(*one)),
        _ => bail!(
            "`{wanted}` names {} {kind}s{scope}; use the {kind}'s id instead",
            named.len()
        ),
    }
}

/// Finds a project of the console by id, or by name when that names exactly one. The console
/// session works from what `list_projects` told it, which is both.
pub(super) fn resolve_project(
    state: &Arc<AppState>,
    console_id: &str,
    wanted: &str,
) -> Result<Project> {
    project_of_console(state, console_id, wanted, "this console")
}

/// The project of console `console_id` that `wanted` names, by id or by a name that names exactly
/// one. `place` words the refusals: "in {place}" for an ambiguous name, "{place} has no project"
/// for none.
fn project_of_console(
    state: &Arc<AppState>,
    console_id: &str,
    wanted: &str,
    place: &str,
) -> Result<Project> {
    let projects = console_projects(state, console_id)?;
    if let Some(project) = projects.iter().find(|project| project.id == wanted) {
        return Ok(project.clone());
    }
    let named = projects.iter().filter(|project| project.name == wanted);
    match unique(named, wanted, "project", &format!(" in {place}"))? {
        Some(project) => Ok(project.clone()),
        None => bail!("{place} has no project `{wanted}`"),
    }
}

/// A project by id in any console, or by a name that is unambiguous in the caller's own console,
/// else in all of them.
pub(super) fn find_project(
    state: &Arc<AppState>,
    caller: &Session,
    wanted: &str,
) -> Result<Project> {
    let projects = state.store.list_projects()?;
    if let Some(project) = projects.iter().find(|project| project.id == wanted) {
        return Ok(project.clone());
    }
    let named = || projects.iter().filter(|project| project.name == wanted);
    let own = named().filter(|project| project.console_id == caller.console_id);
    let found = match unique(own, wanted, "project", "")? {
        Some(project) => Some(project),
        None => unique(named(), wanted, "project", "")?,
    };
    match found {
        Some(project) => Ok(project.clone()),
        None => bail!("there is no project `{wanted}`"),
    }
}

/// [`find_project`] for a read that also names a console: the project is looked for among that
/// console's only, not the caller's first, so `{console: "Other", project: "app"}` means Other's
/// `app`. Without a console it is [`find_project`].
pub(super) fn find_project_in(
    state: &Arc<AppState>,
    caller: &Session,
    console: Option<&Console>,
    wanted: &str,
) -> Result<Project> {
    let Some(console) = console else {
        return find_project(state, caller, wanted);
    };
    project_of_console(
        state,
        &console.id,
        wanted,
        &format!("console `{}`", console.id),
    )
}

/// A console by id, or by a name that is unambiguous.
pub(super) fn find_console(state: &Arc<AppState>, wanted: &str) -> Result<Console> {
    let consoles = state.store.list_consoles()?;
    if let Some(console) = consoles.iter().find(|console| console.id == wanted) {
        return Ok(console.clone());
    }
    let named = consoles.iter().filter(|console| console.name == wanted);
    match unique(named, wanted, "console", "")? {
        Some(console) => Ok(console.clone()),
        None => bail!("there is no console `{wanted}`"),
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
