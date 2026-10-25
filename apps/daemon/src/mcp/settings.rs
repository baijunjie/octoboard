//! The settings tools of the Octoboard MCP server: reading and changing the console's own settings,
//! its projects', and the application's, and reading a project session's own project.
//!
//! A write runs the function the control request of the same name runs, so it is refused and
//! broadcast exactly as the user's own would be, and answers with the record as stored. Scope is
//! kept here: a console session reaches its own console and that console's projects, a project
//! session its own project, and nothing that is the user's alone to change is written (accounts,
//! added agents, approval modes, automatic approval, trusted directories, a project's directory,
//! source, remote or trust consent).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::args::{
    optional_agent, optional_bool, optional_string_list, own_console, own_project, parse_agent,
    present_agent, present_text, raw_text, required_str, resolve_project,
};
use crate::coordinator::{self, ConsoleChanges, ProjectChanges, SettingsChanges};
use crate::git_remote;
use crate::launch_settings::LaunchChange;
use crate::protocol::{
    default_account_name_in, Agent, CodedError, Console, Project, ProjectAccount, Session,
};
use crate::state::AppState;
use crate::worktree_settings;

pub(super) async fn update_project(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let project = resolve_project(
        state,
        &console_session.console_id,
        required_str(arguments, "project")?,
    )?;
    let default_agent = present_agent(arguments, "default_agent")?;
    let changes = ProjectChanges {
        name: raw_text(arguments, "name")?,
        default_agent,
        accounts: project_accounts_argument(state, arguments)?,
        launch: launch_argument(arguments, "launch")?,
        pinned: optional_bool(arguments, "pinned")?,
        tags: optional_string_list(arguments, "tags")?,
        add_tags: optional_string_list(arguments, "add_tags")?.unwrap_or_default(),
        remove_tags: optional_string_list(arguments, "remove_tags")?.unwrap_or_default(),
        description: present_text(arguments, "description")?,
        worktrees: worktrees_argument(arguments)?,
    };
    let updated = coordinator::update_project(state, &project.id, changes).await?;
    describe_project(state, &updated)
}

pub(super) fn get_console(state: &Arc<AppState>, console_session: &Session) -> Result<Value> {
    Ok(describe_console(&own_console(state, console_session)?))
}

pub(super) async fn update_console(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let changes = ConsoleChanges {
        name: raw_text(arguments, "name")?,
        default_agent: optional_agent(arguments, "default_agent")?,
        project_session_agent: present_agent(arguments, "project_session_agent")?,
        accounts: console_accounts_argument(state, arguments)?,
        launch: launch_argument(arguments, "launch")?,
        project_launch: launch_argument(arguments, "project_session_launch")?,
        icon: None,
        console_session_prompt: None,
    };
    let updated = coordinator::update_console(state, &console_session.console_id, changes).await?;
    Ok(describe_console(&updated))
}

pub(super) fn get_settings(state: &Arc<AppState>) -> Result<Value> {
    let accounts = state.store.list_accounts()?;
    let default_names = state.store.default_account_names()?;
    let disabled_defaults = state.store.disabled_default_accounts()?;
    // The default account of each agent is no row, so it is added in front of that agent's others,
    // under the id the tools' `accounts` arguments take for it.
    let listed: Vec<Value> = Agent::ALL
        .iter()
        .flat_map(|agent| {
            let default = json!({
                "id": "default",
                "agent": agent,
                "name": default_account_name_in(&default_names, *agent),
                "default": true,
                "disabled": disabled_defaults.contains(agent.wire_name()),
            });
            let others = accounts
                .iter()
                .filter(|account| account.agent == *agent)
                .map(|account| {
                    json!({
                        "id": account.id,
                        "agent": account.agent,
                        "name": account.name,
                        "default": false,
                        "disabled": account.disabled,
                    })
                });
            std::iter::once(default).chain(others)
        })
        .collect();
    Ok(json!({
        "auto_pull_repositories": state.store.auto_pull_repositories()?,
        "auto_delete_unresumable_archives": state.store.auto_delete_unresumable_archives()?,
        "auto_select_account": state.store.auto_select_account()?,
        "sidebar_order_by_status": state.store.sidebar_order_by_status()?,
        "default_clone_dir": state.store.default_clone_dir()?,
        "worktrees": state.store.worktree_settings()?,
        "accounts": listed,
        "trusted_directories": state.store.trusted_directories()?,
    }))
}

pub(super) fn update_settings(
    state: &Arc<AppState>,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    coordinator::update_settings(
        state,
        SettingsChanges {
            auto_pull_repositories: optional_bool(arguments, "auto_pull_repositories")?,
            auto_delete_unresumable_archives: optional_bool(
                arguments,
                "auto_delete_unresumable_archives",
            )?,
            // None of these is settable by an agent: each would let a session approve its own
            // requests.
            auto_approve_session_requests: None,
            auto_approve_resume_requests: None,
            auto_approve_account_switches: None,
            auto_approve_handoffs: None,
            auto_select_account: optional_bool(arguments, "auto_select_account")?,
            sidebar_order_by_status: optional_bool(arguments, "sidebar_order_by_status")?,
            // Not settable by an agent: it spends the user's quota.
            auto_start_quota_windows: None,
            default_clone_dir: raw_text(arguments, "default_clone_dir")?,
            worktrees: worktrees_argument(arguments)?,
        },
    )?;
    get_settings(state)
}

/// The caller's own project's settings, and nobody else's: what a project session may read of
/// Octoboard's own record of it.
pub(super) fn get_project(state: &Arc<AppState>, caller: &Session) -> Result<Value> {
    describe_project(state, &own_project(state, caller, None)?)
}

/// The optional `worktrees` argument, read as the control requests read theirs: an item left out
/// is left alone, and for a project `null` goes back to following the application-wide value. For
/// a new project, where there is nothing to leave alone, both follow it.
pub(super) fn worktrees_argument<T: serde::de::DeserializeOwned + Default>(
    arguments: &Map<String, Value>,
) -> Result<T> {
    match arguments.get("worktrees") {
        None | Some(Value::Null) => Ok(T::default()),
        Some(value) => {
            // The control protocol ignores keys it does not know; a tool call that misspells an
            // item would look like a success while changing nothing, so it is refused here.
            if let Some(unknown) = value.as_object().and_then(|items| {
                items
                    .keys()
                    .find(|key| !crate::worktree_settings::ITEMS.contains(&key.as_str()))
            }) {
                bail!("`worktrees.{unknown}` is not a worktree setting");
            }
            serde_json::from_value(value.clone())
                .map_err(|err| anyhow!("`worktrees` is not valid: {err}"))
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
fn console_accounts_argument(
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
    let key = coordinator::account_name_key(wanted);
    let accounts = state.store.list_accounts()?;
    let of_agent = || accounts.iter().filter(|account| account.agent == agent);
    of_agent()
        .find(|account| account.id == wanted)
        .or_else(|| of_agent().find(|account| coordinator::account_name_key(&account.name) == key))
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

/// An object keyed by agent, one entry for each.
fn per_agent_object(entry: impl Fn(Agent) -> Value) -> Value {
    Value::Object(
        Agent::ALL
            .iter()
            .map(|agent| (agent.wire_name().to_string(), entry(*agent)))
            .collect(),
    )
}

/// A project's settings as every tool that reports one gives them: the account setting of each
/// agent as `inherit`, `default` or an account id, and the remote without any credential it
/// carries, and its worktree settings as the project's own choices (`null` follows the
/// application-wide value) beside the values in force.
pub(super) fn describe_project(state: &Arc<AppState>, project: &Project) -> Result<Value> {
    Ok(json!({
        "project": project.id,
        "name": project.name,
        "description": project.description,
        "host": project.host_id,
        "path": project.path,
        "source": project.source,
        "remote_url": project.remote_url.as_deref().map(git_remote::without_credentials),
        "default_agent": project.default_agent,
        "pinned": project.pinned,
        "tags": project.tags,
        "trust_consent": project.trust_consent,
        "accounts": per_agent_object(|agent| match project.account(agent) {
            ProjectAccount::Inherit => json!("inherit"),
            ProjectAccount::Default => json!("default"),
            ProjectAccount::Account { id } => json!(id),
        }),
        "launch": per_agent_object(|agent| {
            let (model, effort) = project.launch(agent);
            json!({ "model": model, "effort": effort })
        }),
        "worktrees": {
            "override": project.worktrees,
            "effective": worktree_settings::effective(&state.store, project)?,
        },
    }))
}

/// A console's settings as `get_console` and `update_console` give them. The avatar is a large
/// `data:` URL of no use to the caller, so only whether there is one is said.
pub(super) fn describe_console(console: &Console) -> Value {
    json!({
        "console": console.id,
        "name": console.name,
        "default_agent": console.default_agent,
        "project_session_agent": console.project_session_agent,
        "accounts": per_agent_object(|agent| {
            json!(console.account(agent).as_deref().unwrap_or("default"))
        }),
        "launch": per_agent_object(|agent| {
            let (model, effort) = console.launch(agent);
            json!({ "model": model, "effort": effort })
        }),
        "project_session_launch": per_agent_object(|agent| {
            let (model, effort) = console.project_launch(agent);
            json!({ "model": model, "effort": effort })
        }),
        "has_icon": console.icon.is_some(),
        "workdir": console.workdir,
    })
}
