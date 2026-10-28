//! The settings tools of the Octoboard MCP server: reading and changing the console's own settings,
//! its projects', and the application's, and reading a project session's own project.
//!
//! A write runs the function the control request of the same name runs, so it is refused and
//! broadcast exactly as the user's own would be, and answers with the record as stored. Scope is
//! kept here: a console session reaches its own console and that console's projects, a project
//! session its own project, and nothing that is the user's alone to change is written (accounts,
//! added agents, approval modes, automatic approval, asking questions as choices, the interface
//! language, trusted directories, a project's directory, source, remote or trust
//! consent).

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
use crate::git_workflow;
use crate::launch_settings::LaunchChange;
use crate::protocol::{
    default_account_name_in, Agent, CodedError, Console, Project, ProjectAccount, Session,
};
use crate::state::AppState;

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
        git_workflow: git_workflow_argument(arguments)?,
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
        "git_workflow": state.store.git_workflow_settings()?,
        "claude_compatibility": state.store.claude_compatibility()?,
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
            // Not settable by an agent: it shapes what sessions are told, and an agent must not
            // be able to switch off the rule it is given.
            ask_questions_as_choices: None,
            // Likewise the interface language, which only the user's own client reports.
            interface_language: None,
            auto_select_account: optional_bool(arguments, "auto_select_account")?,
            // Not settable by an agent: it has Octoboard end sessions' processes and spend
            // another account's quota without asking.
            auto_switch_on_usage_limit: None,
            sidebar_order_by_status: optional_bool(arguments, "sidebar_order_by_status")?,
            // Not settable by an agent: it spends the user's quota.
            auto_start_quota_windows: None,
            default_clone_dir: raw_text(arguments, "default_clone_dir")?,
            git_workflow: git_workflow_argument(arguments)?,
            claude_compatibility: claude_compatibility_argument(arguments)?,
        },
    )?;
    get_settings(state)
}

/// The caller's own project's settings, and nobody else's: what a project session may read of
/// Octoboard's own record of it.
///
/// With `refresh`, the project's git status is checked against its remote first (see
/// [`refresh_git_status`]), and the answer says whether the check finished.
pub(super) async fn get_project(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let project = own_project(state, caller, None)?;
    let refreshed = if optional_bool(arguments, "refresh")?.unwrap_or(false) {
        Some(refresh_git_status(state, vec![project.clone()]).await)
    } else {
        None
    };
    let mut description = describe_project(state, &project)?;
    if let Some(refreshed) = refreshed {
        description["git_refresh"] = refreshed;
    }
    Ok(description)
}

/// How long a `refresh` of the git status waits for the checks it asked for. A check that goes
/// to a slow remote can take a minute or more, and the call must not hold its caller for that.
const GIT_REFRESH_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Checks `projects` against their remotes as the application's own refresh does (the same
/// one-minute floor and in-flight claim, and a fast-forward where `auto_pull_repositories` is on),
/// waits up to [`GIT_REFRESH_WAIT`] for the results, and says in `{ finished, unfinished, note }`
/// whether they all came in; the statuses are then read from the daemon as usual.
pub(super) async fn refresh_git_status(state: &Arc<AppState>, projects: Vec<Project>) -> Value {
    refresh_git_status_within(state, projects, GIT_REFRESH_WAIT).await
}

/// [`refresh_git_status`] waiting up to `wait`, which the tests shorten.
pub(super) async fn refresh_git_status_within(
    state: &Arc<AppState>,
    projects: Vec<Project>,
    wait: std::time::Duration,
) -> Value {
    let unfinished = crate::git_status::refresh_and_wait(state, projects, wait).await;
    if unfinished.is_empty() {
        return json!({ "finished": true });
    }
    json!({
        "finished": false,
        "unfinished": unfinished,
        "note": format!(
            "The check of these projects had not finished after {} seconds and goes on in the \
             background; their `git_status` is what the daemon held before, so read it again \
             later.",
            wait.as_secs()
        ),
    })
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

/// The `claude_compatibility` argument: agent wire name to on or off, for the agents it is
/// given. Naming Claude Code is refused by the settings update itself.
fn claude_compatibility_argument(
    arguments: &Map<String, Value>,
) -> Result<std::collections::BTreeMap<String, bool>> {
    per_agent(arguments, "claude_compatibility")?
        .into_iter()
        .map(|(agent, value)| match value {
            Value::Bool(enabled) => Ok((agent.wire_name().to_string(), *enabled)),
            _ => bail!(
                "`claude_compatibility.{}` must be true or false",
                agent.wire_name()
            ),
        })
        .collect()
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

/// A project's git state as the daemon holds it, read from memory without going to the remote:
/// `null` while it holds none (no check has run since it started, e.g. a project of a console never
/// shown), else the `GitStatus` fields except `project` and `checked_seconds_ago`, how long ago
/// the last check completed (`null` when none has since it started, so the counts then come from
/// the local repository alone). A credential in the error `git` reported is removed, as the
/// remote's is.
fn describe_git_status(state: &Arc<AppState>, project: &Project) -> Value {
    let Some((status, checked)) = state.git_status_with_age(&project.id) else {
        return Value::Null;
    };
    json!({
        "repository": status.repository,
        "branch": status.branch,
        "detached": status.detached,
        "upstream": status.upstream,
        "ahead": status.ahead,
        "behind": status.behind,
        "activity": status.activity,
        "error": status.error.as_deref().map(without_credentials_in),
        "checked_seconds_ago": checked.map(|age| age.as_secs()),
    })
}

/// `text` with the credential of any URL in it removed.
fn without_credentials_in(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let url = word.trim_matches(|c: char| c.is_whitespace() || "'\"`(),".contains(c));
            if url.contains("://") && git_remote::has_credentials(url) {
                word.replace(url, &git_remote::without_credentials(url))
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A project's settings as every tool that reports one gives them: the account setting of each
/// agent as `inherit`, `default` or an account id, and the remote without any credential it
/// carries, and its git workflow settings as the project's own choices (`null` follows the
/// application-wide value) beside the values in force, and its git state as last read (see
/// [`describe_git_status`]). The agent, account and launch settings are given as stored, with what
/// they resolve to under the console in `effective`: a project session cannot read the console.
pub(super) fn describe_project(state: &Arc<AppState>, project: &Project) -> Result<Value> {
    let console = state
        .store
        .get_console(&project.console_id)?
        .ok_or_else(|| CodedError::unknown_console(&project.console_id))?;
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
        // The same resolution a launch makes, so that what an agent reads is what a session
        // started in this project gets when nothing names otherwise.
        "effective": {
            "default_agent": coordinator::session_agent(None, Some(project), &console),
            "accounts": per_agent_object(|agent| {
                json!(console
                    .resolve_project_account(agent, project.account(agent))
                    .as_deref()
                    .unwrap_or("default"))
            }),
            "launch": per_agent_object(|agent| {
                let (model, effort) =
                    coordinator::project_launch_settings(project, &console, agent);
                json!({ "model": model, "effort": effort })
            }),
        },
        "git_workflow": {
            "override": project.git_workflow,
            "effective": git_workflow::effective(&state.store, project)?,
        },
        "git_status": describe_git_status(state, project),
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
