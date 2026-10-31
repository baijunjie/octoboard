//! Creating, changing and deleting consoles.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use crate::agents::launch_settings::{self, normalized_launches, trimmed_or_none, LaunchCheck};
use crate::mcp;
use crate::protocol::{
    error_code, now_millis, Agent, CodedError, Console, Event, Launch, LaunchChange, PerAgent,
    Present, Project,
};
use crate::state::AppState;

use super::account_choice::checked_account_reference;
use super::lifecycle::{ensure_unlocked, session_ids_where};
use super::validate::{normalize_icon, required_text};

/// A console to create, as the request carries it: account references by id, models and efforts
/// as typed.
pub struct NewConsole {
    pub name: String,
    pub default_agent: Agent,
    pub project_session_agent: Option<Agent>,
    pub accounts: PerAgent<Option<String>>,
    pub launch: PerAgent<Launch>,
    pub project_session_launch: PerAgent<Launch>,
    pub icon: Option<String>,
    pub console_session_prompt: Option<String>,
}

/// Every refusal `create_console` can make of `new`, with nothing written. The one place that
/// list lives: a caller that has to know a console will not be refused before it creates anything
/// asks here rather than repeating the checks. Returns the avatar as it is stored.
pub fn check_new_console(state: &Arc<AppState>, new: &NewConsole) -> Result<Option<String>> {
    required_text("name", &new.name)?;
    let icon = normalize_icon(new.icon.as_deref())?;
    for (agent, account) in new.accounts.iter() {
        checked_account_reference(state, agent, account.clone())?;
    }
    Ok(icon)
}

/// Refuses a console session model or effort in `new` that the agent does not offer (see
/// `launch_settings`). Separate from [`check_new_console`], which has no wait in it, and left out
/// for a console a configuration document creates, whose values were chosen on another machine.
pub async fn check_new_console_launch(state: &Arc<AppState>, new: &NewConsole) -> Result<()> {
    for set in [&new.launch, &new.project_session_launch] {
        for (agent, launch) in set.iter() {
            let account = new.accounts.get(agent).clone();
            LaunchCheck::given(
                agent,
                account,
                launch.model.as_deref(),
                launch.effort.as_deref(),
            )
            .run(state)
            .await?;
        }
    }
    Ok(())
}

/// Creates and broadcasts a console from a configuration document, whose model and effort values
/// are stored as given rather than checked against what the agent offers. Every other field is
/// checked first, so a refusal leaves no working directory behind.
pub fn create_console(state: &Arc<AppState>, new: NewConsole) -> Result<Console> {
    let icon = check_new_console(state, &new)?;
    store_new_console(state, new, icon)
}

/// Stores and broadcasts a console [`check_new_console`] accepted, `icon` being what it returned.
pub(super) fn store_new_console(
    state: &Arc<AppState>,
    new: NewConsole,
    icon: Option<String>,
) -> Result<Console> {
    let id = Uuid::new_v4().to_string();
    let workdir = state.console_workdir(&id);
    std::fs::create_dir_all(&workdir)?;
    let console = Console {
        id,
        name: new.name.trim().to_string(),
        workdir: workdir.to_string_lossy().into_owned(),
        default_agent: new.default_agent,
        project_session_agent: new.project_session_agent,
        accounts: new.accounts,
        launch: normalized_launches(new.launch),
        project_session_launch: normalized_launches(new.project_session_launch),
        icon,
        console_session_prompt: trimmed_or_none(new.console_session_prompt.as_deref()),
        created_at: now_millis(),
    };
    // Written before the record is stored: nothing else can touch a console no one has been told
    // of yet, and a failed write then leaves no console behind.
    mcp::console_instructions::write_console_session_instructions_from_settings(
        &state.store,
        &console,
    )?;
    state.store.insert_console(&console)?;
    state.broadcast(Event::ConsoleUpserted {
        console: console.clone(),
    });
    Ok(console)
}

/// What `update_console` changes; a field left at its default is left alone.
#[derive(Default)]
pub struct ConsoleChanges {
    pub name: Option<String>,
    pub default_agent: Option<Agent>,
    /// `Some(None)` makes project sessions follow `default_agent` again.
    pub project_session_agent: Present<Agent>,
    /// Each agent's account by id; `Some(None)` is back to that agent's default account.
    pub accounts: HashMap<Agent, Option<String>>,
    pub launch: HashMap<Agent, LaunchChange>,
    /// Each agent's project session model and effort.
    pub project_launch: HashMap<Agent, LaunchChange>,
    /// `Some(None)` clears the avatar.
    pub icon: Present<String>,
    /// `Some(None)` clears the user's addition to the console session's instructions.
    pub console_session_prompt: Present<String>,
}

/// Applies `changes` to the console `id`, broadcasts it and returns it as stored. Every field is
/// checked first, with nothing written yet. An absent reference is not checked at all: saving a
/// console with a field untouched succeeds whatever has happened to the account it names since, and
/// a model or effort that is not being changed is not checked against the agent's list either.
///
/// Asking the agent for its models can take a while, so it is done before the record is read for
/// the write: the change is applied to the console as it is once the answers are in, and the
/// checks are made again only if an edit meanwhile moved the account one of them was made under.
pub async fn update_console(
    state: &Arc<AppState>,
    id: &str,
    changes: ConsoleChanges,
) -> Result<Console> {
    let name = changes
        .name
        .as_deref()
        .map(|name| required_text("name", name))
        .transpose()?;
    let icon = changes
        .icon
        .as_ref()
        .map(|icon| normalize_icon(icon.as_deref()))
        .transpose()?;
    let console_session_prompt = changes
        .console_session_prompt
        .as_ref()
        .map(|prompt| trimmed_or_none(prompt.as_deref()));
    let mut accounts = HashMap::new();
    for (agent, reference) in &changes.accounts {
        accounts.insert(
            *agent,
            checked_account_reference(state, *agent, reference.clone())?,
        );
    }
    let mut verified = Vec::new();
    loop {
        let mut console = state
            .store
            .get_console(id)?
            .ok_or_else(|| CodedError::unknown_console(id))?;
        if let Some(name) = &name {
            console.name = name.clone();
        }
        if let Some(default_agent) = changes.default_agent {
            console.default_agent = default_agent;
        }
        if let Some(project_session_agent) = changes.project_session_agent {
            console.project_session_agent = project_session_agent;
        }
        // Only sessions opened afterwards take a new reference; each existing one keeps the
        // account and the directory it was started with (see `Session::account_id`).
        for (agent, reference) in &accounts {
            *console.accounts.get_mut(*agent) = reference.clone();
        }
        // Read when a console session is opened or resumed; one already running keeps the
        // values it was started with.
        let mut pending = Vec::new();
        for (agent, change) in &changes.launch {
            let account_id = console.accounts.get(*agent).clone();
            let launch = console.launch.get_mut(*agent);
            let check = launch_settings::apply_change(*agent, account_id, launch, change);
            if !check.is_empty() && !verified.contains(&check) {
                pending.push(check);
            }
        }
        for (agent, change) in &changes.project_launch {
            let account_id = console.accounts.get(*agent).clone();
            let launch = console.project_session_launch.get_mut(*agent);
            let check = launch_settings::apply_change(*agent, account_id, launch, change);
            if !check.is_empty() && !verified.contains(&check) {
                pending.push(check);
            }
        }
        if !pending.is_empty() {
            for check in pending {
                check.run(state).await?;
                verified.push(check);
            }
            continue;
        }
        if let Some(icon) = &icon {
            console.icon = icon.clone();
        }
        if let Some(prompt) = &console_session_prompt {
            console.console_session_prompt = prompt.clone();
        }
        state.store.update_console(&console)?;
        // Read back rather than sent as `console`: the record is what clients are told it is.
        let stored = state
            .store
            .get_console(id)?
            .ok_or_else(|| CodedError::unknown_console(id))?;
        state.broadcast(Event::ConsoleUpserted {
            console: stored.clone(),
        });
        // After the record is stored, so the file never holds text that was not saved. Every
        // agent's new console sessions read the change, and a resumed one does on Claude Code and
        // Codex, never on Grok. A failure here is logged, not returned, since the change is saved
        // and told to every client: the next launch's refresh writes the file.
        if let Err(err) =
            mcp::console_instructions::refresh_console_session_instructions(&state.store, id)
        {
            tracing::warn!(console = %id, %err, "refreshing the console's instruction file failed");
        }
        return Ok(stored);
    }
}

/// Deletes the console `id` with its projects and sessions, refused while one of its sessions is
/// locked, running or still starting.
pub(super) async fn delete_console(state: &Arc<AppState>, id: String) -> Result<()> {
    let _removal = state.claim_session_removal();
    ensure_unlocked(
        state
            .store
            .list_sessions()?
            .into_iter()
            .filter(|session| session.console_id == id),
    )?;
    let console = state
        .store
        .get_console(&id)?
        .ok_or_else(|| CodedError::unknown_console(&id))?;
    if state.has_live_sessions_where(|session| session.console_id == id)? {
        return Err(CodedError::raised(
            error_code::CONSOLE_HAS_RUNNING_SESSIONS,
            "this console still has running or starting sessions; archive the running ones, or wait for the starting ones to finish",
            &[],
        ));
    }
    // Read before the delete below: `projects.console_id` is `ON DELETE CASCADE`, so the
    // store drops this console's projects with it, and nothing else would ever clean up
    // their git statuses afterwards — the daemon keeps no timer of its own that would
    // notice they are gone.
    let projects: Vec<Project> = state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project.console_id == id)
        .collect();
    // Its sessions cascade the same way, and their saved output is Octoboard's own.
    let session_ids = session_ids_where(state, |session| session.console_id == id)?;
    state.store.delete_console(&id)?;
    let project_ids: Vec<&str> = projects.iter().map(|project| project.id.as_str()).collect();
    state.forget_deleted_projects(&project_ids);
    state.forget_cascaded_sessions(&session_ids);
    state.stalled_directory_passes.forget(&id);
    for project in &projects {
        crate::branch_origin::project_deleted(state, &project.path);
    }
    crate::terminal::shell::close_projects(state, &project_ids).await;
    // Their processes, which no session owns, go with the projects.
    crate::processes::requests::stop_project_processes(state, &projects).await;
    // The working directory is Octoboard's own, under `~/.octoboard/consoles/`; no project
    // directory is ever touched by this.
    if let Err(err) = mcp::console_instructions::remove_console_workdir(&console.workdir) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(workdir = %console.workdir, %err, "removing the console working directory failed");
        }
    }
    state.broadcast(Event::ConsoleDeleted { console: id });
    Ok(())
}

#[cfg(test)]
mod tests;
