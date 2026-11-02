//! Reading a tool call's arguments, and finding the records they name within the caller's reach:
//! projects, consoles, sessions and accounts, and the bounds of a page (`page_bounds`).
//!
//! A malformed boolean, string list or text argument is refused with a reason rather than read as
//! absent: a `"false"` that quietly changed nothing would look like success to the model that sent
//! it. A project named for a write (`resolve_project`) is found only among the caller's own
//! console's, so an id from another console resolves to nothing; the reads (`find_project`) take
//! an id from any console, and a name from the caller's console first, else from all. A session is
//! read by any caller, but acted on only when it is bound to the caller (`resolve_owned_session`).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

use crate::protocol::{
    Agent, CodedError, Console, LaunchChange, Project, ProjectAccount, Role, Session, SessionStatus,
};
use crate::state::AppState;

pub(super) fn required_str<'a>(arguments: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("`{key}` is required"))
}

pub(super) fn optional_string(arguments: &Map<String, Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// An optional text argument, trimmed: `None` when absent, `null` or blank, refused when anything
/// else but a string.
pub(super) fn optional_text(arguments: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    if matches!(arguments.get(key), Some(Value::Null)) {
        return Ok(None);
    }
    Ok(raw_text(arguments, key)?
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty()))
}

pub(super) fn optional_agent(arguments: &Map<String, Value>, key: &str) -> Result<Option<Agent>> {
    optional_string(arguments, key)
        .map(|value| parse_agent(&value))
        .transpose()
}

pub(super) fn parse_agent(value: &str) -> Result<Agent> {
    Agent::from_wire_name(value).ok_or_else(|| {
        let names: Vec<String> = Agent::ALL
            .iter()
            .map(|agent| format!("`{}`", agent.wire_name()))
            .collect();
        let (last, rest) = names.split_last().expect("there are agents");
        anyhow!("`{value}` is not one of {} or {last}", rest.join(", "))
    })
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

/// The part of `len` items that a page of `limit` items from `offset` covers, as `start..end`, and
/// the `offset` of the next page, or `None` when this one reaches the end. An `offset` past the
/// end gives an empty page at the end.
pub(super) fn page_bounds(
    len: usize,
    offset: usize,
    limit: usize,
) -> (usize, usize, Option<usize>) {
    let end = offset.saturating_add(limit).min(len);
    let start = offset.min(end);
    (start, end, (end < len).then_some(end))
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

/// The optional `git_workflow` argument, read as the control requests read theirs: an item left out
/// is left alone, and for a project `null` goes back to following the application-wide value. For
/// a new project, where there is nothing to leave alone, both follow it.
pub(super) fn git_workflow_argument<T: serde::de::DeserializeOwned + Default>(
    arguments: &Map<String, Value>,
) -> Result<T> {
    match arguments.get("git_workflow") {
        None | Some(Value::Null) => Ok(T::default()),
        Some(value) => {
            // The control protocol ignores keys it does not know; a tool call that misspells an
            // item would look like a success while changing nothing, so it is refused here.
            if let Some(unknown) = value.as_object().and_then(|items| {
                items
                    .keys()
                    .find(|key| !crate::git_workflow::ITEMS.contains(&key.as_str()))
            }) {
                bail!("`git_workflow.{unknown}` is not a git workflow setting");
            }
            serde_json::from_value(value.clone())
                .map_err(|err| anyhow!("`git_workflow` is not valid: {err}"))
        }
    }
}

/// The entries of an argument that is an object keyed by agent, in no particular order.
fn per_agent<'a>(arguments: &'a Map<String, Value>, key: &str) -> Result<Vec<(Agent, &'a Value)>> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Object(entries)) => entries
            .iter()
            .map(|(agent, value)| Ok((parse_agent(agent)?, value)))
            .collect(),
        Some(_) => bail!("`{key}` must be an object keyed by agent"),
    }
}

/// The `accounts` argument of a project tool: `inherit`, `default`, or an account of that agent.
pub(super) fn project_accounts_argument(
    state: &Arc<AppState>,
    arguments: &Map<String, Value>,
) -> Result<HashMap<Agent, ProjectAccount>> {
    per_agent(arguments, "accounts")?
        .into_iter()
        .map(|(agent, value)| {
            let wanted = value
                .as_str()
                .ok_or_else(|| anyhow!("`accounts.{}` must be a string", agent.wire_name()))?;
            let account = if wanted.trim().eq_ignore_ascii_case("inherit") {
                ProjectAccount::Inherit
            } else {
                match resolve_account(state, agent, wanted)? {
                    None => ProjectAccount::Default,
                    Some(id) => ProjectAccount::Account { id },
                }
            };
            Ok((agent, account))
        })
        .collect()
}

/// The `accounts` argument of `update_console`: `default` or `null`, or an account of that agent.
pub(super) fn console_accounts_argument(
    state: &Arc<AppState>,
    arguments: &Map<String, Value>,
) -> Result<HashMap<Agent, Option<String>>> {
    per_agent(arguments, "accounts")?
        .into_iter()
        .map(|(agent, value)| match value {
            Value::Null => Ok((agent, None)),
            Value::String(wanted) => Ok((agent, resolve_account(state, agent, wanted)?)),
            _ => bail!("`accounts.{}` must be a string or null", agent.wire_name()),
        })
        .collect()
}

/// The account `wanted` names for `agent` — `default` for the agent's default account (`None`),
/// else an id or a name among that agent's accounts. Another agent's account is refused here as an
/// unknown one, which is what it is for this agent.
pub(super) fn resolve_account(
    state: &Arc<AppState>,
    agent: Agent,
    wanted: &str,
) -> Result<Option<String>> {
    let wanted = wanted.trim();
    if wanted.eq_ignore_ascii_case("default") {
        return Ok(None);
    }
    let key = crate::accounts::account_name_key(wanted);
    let accounts = state.store.list_accounts()?;
    let of_agent = || accounts.iter().filter(|account| account.agent == agent);
    of_agent()
        .find(|account| account.id == wanted)
        .or_else(|| {
            of_agent().find(|account| crate::accounts::account_name_key(&account.name) == key)
        })
        .map(|account| Some(account.id.clone()))
        .ok_or_else(|| CodedError::unknown_account(wanted))
}

/// The argument named `key` (`launch`, `project_session_launch`): each agent's model and effort,
/// either of which may be left out.
pub(super) fn launch_argument(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<HashMap<Agent, LaunchChange>> {
    per_agent(arguments, key)?
        .into_iter()
        .map(|(agent, value)| {
            let fields = value
                .as_object()
                .ok_or_else(|| anyhow!("`{key}.{}` must be an object", agent.wire_name()))?;
            Ok((
                agent,
                LaunchChange {
                    model: present_text(fields, "model")?,
                    effort: present_text(fields, "effort")?,
                },
            ))
        })
        .collect()
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

/// Finds a session the caller may read: any session on record, of any console or project. What a
/// session may *do* to another is narrower, and is [`resolve_owned_session`]'s.
pub(super) fn resolve_session(state: &Arc<AppState>, wanted: &str) -> Result<Session> {
    state
        .store
        .get_session(wanted)?
        .ok_or_else(|| anyhow!("there is no session `{wanted}`"))
}

/// The session a read tool's optional `session` argument names, the caller itself when it is left
/// out (or names the caller), read afresh from the store. Reading, like [`resolve_session`].
pub(super) fn session_or_caller(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Session> {
    match optional_string(arguments, "session") {
        Some(wanted) if wanted != caller.id => resolve_session(state, &wanted),
        _ => state.session_record(&caller.id),
    }
}

/// Finds a session the caller may act on: one it can read that is bound to it. Every tool that
/// writes into another session goes through this and nothing else, so the rule that reads are wide
/// and writes are the caller's own is kept in one place. A session that is unbound, or bound to
/// another session, is refused with a reason (naming the owner when there is one), so the model
/// leaves it alone rather than retrying.
pub(super) fn resolve_owned_session(
    state: &Arc<AppState>,
    caller: &Session,
    wanted: &str,
) -> Result<Session> {
    let session = resolve_session(state, wanted)?;
    // `send_message` refuses an archived session, so it is not offered for one.
    let send_message =
        crate::session_tools::catalogue::qualified_tool_name(caller.agent, "send_message");
    let archived = session.status == SessionStatus::Archived;
    let tell_it = |tail: &str| {
        if archived {
            String::new()
        } else {
            format!(
                " You can still tell it something with `{send_message}`, which it weighs rather \
                 than obeys{tail}"
            )
        }
    };
    // Including a console session's own: one that ended itself would leave its console's project
    // sessions reporting to nothing.
    if wanted == caller.id {
        bail!(
            "session `{wanted}` is your own session, which is not yours to act on; if you meant \
             to say something, say it to the user"
        );
    }
    if session.role == Role::Console {
        bail!(
            "session `{wanted}` is a console session; acting on one is the user's to do, not \
             yours.{}",
            tell_it("")
        );
    }
    match session.bound_to.as_deref() {
        Some(owner) if owner == caller.id => Ok(session),
        Some(owner) => {
            let owner_record = state.store.get_session(owner)?;
            // "Another" only when the owner is the caller's own kind: a project session is not
            // told that a console session is another one of its kind.
            let kind = owner_record
                .as_ref()
                .map_or("session".to_string(), |owner| {
                    let article = if owner.role == caller.role {
                        "another"
                    } else {
                        "a"
                    };
                    format!("{article} {} session", owner.role.kind())
                });
            let title = owner_record
                .map(|owner| format!(" (\"{}\")", owner.title))
                .unwrap_or_default();
            bail!(
                "session `{wanted}` is bound to {kind}, `{owner}`{title}. Only that session \
                 instructs or drives it, so this is not yours to do.{}",
                tell_it(", and its owner is sent a copy")
            )
        }
        None => bail!(
            "session `{wanted}` is not bound to any session: the user opened it themselves and \
             kept it outside the orchestration. It is theirs, not yours to drive.{}",
            tell_it("")
        ),
    }
}

#[cfg(test)]
mod tests;
