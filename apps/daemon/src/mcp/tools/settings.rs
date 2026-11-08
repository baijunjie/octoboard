//! The settings tools of the Octoboard MCP server: reading and changing the console's own settings,
//! its projects', and the application's, adding a project, and reading a project session's own
//! project.
//!
//! A write runs the function the control request of the same name runs, so it is refused and
//! broadcast exactly as the user's own would be, and answers with the record as stored. Scope is
//! kept here: a console session reaches its own console and that console's projects, a project
//! session its own project. The settings of "Agent behavior" and "Agent accounts" (automatic
//! approvals, asking questions as choices, accounts, added agents, automatic account selection and
//! switching, starting usage windows, the providers' settings, agent tags), the permission settings
//! (approval modes, trusted directories, a project's trust consent) and a console's session
//! instructions are the user's alone and never written here. Of the Agent accounts settings, the
//! accounts, `auto_select_account`, the providers' settings that declare `shown_to_agents`
//! (`provider_settings`) and `agent_tags` are returned read-only, as are the trusted directories. A
//! project's directory, source and remote are chosen when it is added and fixed afterwards.

use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use crate::accounts::choice;
use crate::coordinator::{
    self, consoles::ConsoleChanges, projects::ProjectChanges, settings::SettingsChanges,
};
use crate::mcp::lookup::{
    console_accounts_argument, git_workflow_argument, launch_argument, optional_agent,
    optional_bool, optional_string, optional_string_list, own_console, own_project, present_agent,
    present_text, project_accounts_argument, raw_text, required_str, resolve_project,
};
use crate::mcp::views::{describe_console, describe_project};
use crate::project_git::remote_url;
use crate::protocol::{error_code, AgentId, CodedError, Project, ProjectSource, Session};
use crate::state::AppState;

pub(in crate::mcp) async fn update_project(
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
    let updated = coordinator::projects::update_project(state, &project.id, changes).await?;
    describe_project(state, &updated)
}

pub(in crate::mcp) async fn add_project(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let source = match required_str(arguments, "source")? {
        "local" => ProjectSource::Local,
        "parent" => ProjectSource::Parent,
        "git" => ProjectSource::Git,
        other => bail!("`{other}` is not one of `local`, `parent` or `git`"),
    };
    let remote_url = optional_string(arguments, "remote_url");
    if source == ProjectSource::Git {
        check_clone_url(remote_url.as_deref())?;
    }
    let mut settings = coordinator::projects::NewProjectSettings {
        pinned: optional_bool(arguments, "pinned")?.unwrap_or(false),
        git_workflow: git_workflow_argument(arguments)?,
        ..Default::default()
    };
    for (agent, account) in project_accounts_argument(state, arguments)? {
        *settings.accounts.get_mut(agent) = account;
    }
    for (agent, change) in launch_argument(arguments, "launch")? {
        let launch = settings.launch.get_mut(agent);
        launch.model = change.model.flatten();
        launch.effort = change.effort.flatten();
    }
    let added = coordinator::projects::add_project(
        state,
        coordinator::projects::AddProjectRequest {
            console_id: console_session.console_id.clone(),
            source,
            path: optional_string(arguments, "path"),
            remote_url,
            name: optional_string(arguments, "name"),
            default_agent: optional_agent(arguments, "default_agent")?,
            settings,
            detect_default_agent: true,
            check_launch: true,
            check_text_limits: true,
            create_directory: true,
            tags: optional_string_list(arguments, "tags")?,
            description: present_text(arguments, "description")?.flatten(),
        },
    )
    .await?;
    let projects = added
        .iter()
        .map(|project| describe_project(state, project))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({ "projects": projects }))
}

/// Refuses a repository URL the console session may not clone from: one of a shape the import of a
/// configuration document refuses too, so a tool call is no way to reach a path of this machine or
/// run a helper transport, and one with a credential in it, which would be stored with the project
/// and shown in every listing. The user's own git credentials are what a clone uses.
fn check_clone_url(url: Option<&str>) -> Result<()> {
    let Some(url) = url else { return Ok(()) };
    let shown = remote_url::without_credentials(url);
    if remote_url::has_credentials(url) {
        return Err(CodedError::raised(
            error_code::INVALID_REMOTE_URL,
            format!(
                "`{shown}` carries credentials, which are not accepted in a URL: give it without, \
                 and the user's own git credentials are used"
            ),
            &[("url", &shown)],
        ));
    }
    if !remote_url::is_clonable_url(url) {
        return Err(CodedError::raised(
            error_code::INVALID_REMOTE_URL,
            format!("`{shown}` is not an http(s), ssh or git repository URL that can be cloned"),
            &[("url", &shown)],
        ));
    }
    Ok(())
}

pub(in crate::mcp) fn get_console(
    state: &Arc<AppState>,
    console_session: &Session,
) -> Result<Value> {
    Ok(describe_console(&own_console(state, console_session)?))
}

pub(in crate::mcp) async fn update_console(
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
        icon: present_text(arguments, "icon")?,
        // The user's addition to the console session's instructions is theirs alone: it shapes
        // what console sessions are told.
        console_session_prompt: None,
    };
    let updated =
        coordinator::consoles::update_console(state, &console_session.console_id, changes).await?;
    Ok(describe_console(&updated))
}

pub(in crate::mcp) fn get_settings(state: &Arc<AppState>) -> Result<Value> {
    // The default account of each agent is no row, so it is listed under the id the tools'
    // `accounts` arguments take for it. Each agent's accounts come in the order automatic account
    // selection weighs them.
    let mut listed = Vec::new();
    for agent in AgentId::ALL {
        for candidate in choice::candidate_accounts(state, agent)? {
            listed.push(match candidate.id {
                None => json!({
                    "id": "default",
                    "agent": agent,
                    "name": candidate.name,
                    "default": true,
                    "disabled": candidate.disabled,
                }),
                Some(id) => json!({
                    "id": id,
                    "agent": agent,
                    "name": candidate.name,
                    "default": false,
                    "disabled": candidate.disabled,
                }),
            });
        }
    }
    // A removed agent keeps its tags in the store, but a session is not shown them; every agent's
    // are shown while the added list is undecided.
    let mut agent_tags = Map::new();
    for (name, tags) in state.store.agent_tags()? {
        let shown = match AgentId::from_wire_name(&name) {
            Some(agent) => state.store.agent_is_added(agent)?,
            None => false,
        };
        if shown {
            agent_tags.insert(name, json!(tags));
        }
    }
    Ok(json!({
        "auto_close_idle_shell_terminals": state.store.auto_close_idle_shell_terminals()?,
        "auto_pull_repositories": state.store.auto_pull_repositories()?,
        "auto_delete_unresumable_archives": state.store.auto_delete_unresumable_archives()?,
        "resume_sessions_on_start": state.store.resume_sessions_on_start()?,
        "auto_select_account": state.store.auto_select_account()?,
        "interface_language": state.store.interface_language()?,
        "sidebar_order_by_status": state.store.sidebar_order_by_status()?,
        "default_clone_dir": state.store.default_clone_dir()?,
        "git_workflow": state.store.git_workflow_settings()?,
        "provider_settings": shown_provider_settings(state)?,
        "agent_tags": agent_tags,
        "accounts": listed,
        "trusted_directories": state.store.trusted_directories()?,
    }))
}

/// The values in effect of the provider settings agents may read, by provider id and then setting
/// id; a provider with none to show has no entry.
fn shown_provider_settings(state: &Arc<AppState>) -> Result<Value> {
    let shown = crate::providers::settings_where(state.store.provider_settings()?, |setting, _| {
        setting.shown_to_agents
    });
    Ok(json!(shown))
}

pub(in crate::mcp) fn update_settings(
    state: &Arc<AppState>,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    coordinator::settings::update_settings(
        state,
        SettingsChanges {
            auto_pull_repositories: optional_bool(arguments, "auto_pull_repositories")?,
            auto_close_idle_shell_terminals: optional_bool(
                arguments,
                "auto_close_idle_shell_terminals",
            )?,
            auto_delete_unresumable_archives: optional_bool(
                arguments,
                "auto_delete_unresumable_archives",
            )?,
            // The settings of Settings' "Agent behavior" and "Agent accounts" sections are the
            // user's alone: an agent must not approve its own requests, loosen the rules it is
            // given, or decide which accounts and agents spend the user's quota and how.
            auto_approve_session_requests: None,
            auto_approve_resume_requests: None,
            auto_approve_account_switches: None,
            auto_approve_handoffs: None,
            ask_questions_as_choices: None,
            auto_select_account: None,
            auto_switch_on_usage_limit: None,
            auto_start_quota_windows: None,
            resume_sessions_on_start: optional_bool(arguments, "resume_sessions_on_start")?,
            provider_settings: Default::default(),
            agent_tags: Default::default(),
            interface_language: raw_text(arguments, "interface_language")?
                .map(|tag| tag.trim().to_string()),
            sidebar_order_by_status: optional_bool(arguments, "sidebar_order_by_status")?,
            default_clone_dir: raw_text(arguments, "default_clone_dir")?,
            git_workflow: git_workflow_argument(arguments)?,
        },
    )?;
    get_settings(state)
}

/// The caller's own project's settings, and nobody else's: what a project session may read of
/// Octoboard's own record of it.
///
/// With `refresh`, the project's git status is checked against its remote first (see
/// [`refresh_git_status`]), and the answer says whether the check finished.
pub(in crate::mcp) async fn get_project(
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
async fn refresh_git_status_within(
    state: &Arc<AppState>,
    projects: Vec<Project>,
    wait: std::time::Duration,
) -> Value {
    let unfinished = crate::project_git::status::refresh_and_wait(state, projects, wait).await;
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

#[cfg(test)]
mod tests;
