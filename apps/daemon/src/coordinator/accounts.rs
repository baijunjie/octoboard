//! The accounts and the agent list: names, directories, sign-ins, quotas, and adding, removing
//! and ordering them.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use crate::accounts::dirs::{account_dir_key, normalize_account_dir};
use crate::accounts::login;
use crate::accounts::quotas;
use crate::accounts::{account_name_key, clears_default_account_name};
use crate::adapter;
use crate::agents::availability;
use crate::hostfs;
use crate::protocol::{
    error_code, Account, Agent, Availability, CodedError, Console, Event, Project,
};
use crate::state::{broadcast_settings, AppState};
use crate::store::ReorderOutcome;

use super::account_choice::{
    account_of_agent, checked_account_reference, refuse_if_agent_unavailable,
};
use super::validate::field_required;

/// The name of the account whose name collides with `name` within `agent`, if any — the default
/// account's name takes part, compared against [`crate::protocol::DEFAULT_ACCOUNT_NAME`] since the
/// daemon carries no locale to compare the user's own translated word against. `exclude_id` lets a
/// rename compare against every *other* account without colliding with itself.
pub(crate) fn colliding_account_name(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<Option<String>> {
    let wanted = account_name_key(name);
    if wanted == account_name_key(crate::protocol::DEFAULT_ACCOUNT_NAME) {
        return Ok(Some(crate::protocol::DEFAULT_ACCOUNT_NAME.to_string()));
    }
    if let Some(custom) = state
        .store
        .default_account_names()?
        .get(agent.wire_name())
        .filter(|custom| account_name_key(custom) == wanted)
    {
        return Ok(Some(custom.clone()));
    }
    colliding_owned_account_name(state, agent, name, exclude_id)
}

/// The name of the account of `agent` that is a row (so not the default) whose name collides with
/// `name`, if any.
pub(crate) fn colliding_owned_account_name(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<Option<String>> {
    let wanted = account_name_key(name);
    for existing in state.store.list_accounts()? {
        if existing.agent == agent
            && Some(existing.id.as_str()) != exclude_id
            && account_name_key(&existing.name) == wanted
        {
            return Ok(Some(existing.name));
        }
    }
    Ok(None)
}

/// Names the agent's default account, or clears the name when `name` is blank or is the word the
/// daemon shows for an unnamed default account, in any letter case. The name takes part in the
/// same collision check as an account's, so neither can take the other's; the default's own name
/// is no collision, so only the accounts that are rows are compared.
pub(crate) fn set_default_account_name(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
) -> Result<()> {
    let _writes = state.lock_account_writes();
    write_default_account_name(state, agent, name.trim())
}

/// [`set_default_account_name`] for a default account that still has no name of its own, checked
/// under the lock the write takes so a rename made in between is not overwritten. Whether the name
/// was written: `false` when the default account was named already, or when `name` is blank or
/// the word shown for an unnamed one, which clears the name and so writes nothing here.
pub(crate) fn name_unnamed_default_account(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
) -> Result<bool> {
    let name = name.trim();
    if clears_default_account_name(name) {
        return Ok(false);
    }
    let _writes = state.lock_account_writes();
    if state
        .store
        .default_account_names()?
        .contains_key(agent.wire_name())
    {
        return Ok(false);
    }
    write_default_account_name(state, agent, name)?;
    Ok(true)
}

/// Writes the trimmed `name` of `agent`'s default account; the caller holds the account writes
/// lock.
fn write_default_account_name(state: &Arc<AppState>, agent: Agent, name: &str) -> Result<()> {
    let changed = if clears_default_account_name(name) {
        state.store.set_default_account_name(agent, None)?
    } else {
        if let Some(existing) = colliding_owned_account_name(state, agent, name, None)? {
            return Err(account_name_taken(agent, name, &existing));
        }
        state.store.set_default_account_name(agent, Some(name))?
    };
    if changed {
        broadcast_settings(state)?;
    }
    Ok(())
}

/// Turns an account off for new work, or back on, and broadcasts the settings when the flag
/// changed. `None` is `agent`'s default account. A running session is not looked at: it keeps its
/// account. Turning an account back on starts the usage window its held reading shows unstarted,
/// when the setting that does that is on ([`quotas::kick_held`]).
pub(super) fn set_account_disabled(
    state: &Arc<AppState>,
    agent: Agent,
    account: Option<&str>,
    disabled: bool,
) -> Result<()> {
    let _writes = state.lock_account_writes();
    let changed = match account {
        None => state.store.set_default_account_disabled(agent, disabled)?,
        Some(id) => {
            if account_of_agent(state, agent, id)?.is_none() {
                return Err(CodedError::unknown_account(id));
            }
            state.store.set_account_disabled(id, disabled)?
        }
    };
    if changed {
        broadcast_settings(state)?;
        // The held readings are looked over again, as when the setting that starts quota windows
        // is turned on, so a window the re-enabled account shows unstarted is not left waiting.
        if !disabled {
            quotas::kick_held(state);
        }
    }
    Ok(())
}

/// Refuses to put a new session on an account the user turned off, going by the flag as it stands.
/// Applies to every origin, the user's own request included; a session already on the account is
/// never asked about.
pub(crate) fn refuse_if_account_disabled(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<()> {
    if !state.store.account_disabled(agent, account_id)? {
        return Ok(());
    }
    let account = state.store.account_display_name(agent, account_id)?;
    Err(CodedError::raised(
        error_code::ACCOUNT_DISABLED,
        format!(
            "{account} is disabled for {}. Choose another account, or turn it back on in Settings \
             → Agent accounts.",
            agent.label()
        ),
        &[("agent", agent.label()), ("account", account.as_str())],
    ))
}

/// Refuses a name that collides within its agent, naming the account it collides with.
fn check_account_name_unique(
    state: &Arc<AppState>,
    agent: Agent,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<()> {
    match colliding_account_name(state, agent, name, exclude_id)? {
        Some(existing) => Err(account_name_taken(agent, name, &existing)),
        None => Ok(()),
    }
}

pub(crate) fn account_name_taken(agent: Agent, name: &str, existing: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::ACCOUNT_NAME_TAKEN,
        format!(
            "the name `{name}` is already used by `{existing}` for {}",
            agent.label()
        ),
        &[("agent", agent.label()), ("name", existing)],
    )
}

/// The name of the account whose directory is `dir` (as [`normalize_account_dir`] stores it)
/// within `agent`, if any. The default account takes part under
/// [`crate::protocol::DEFAULT_ACCOUNT_NAME`], by the directory the daemon resolved when it
/// determined availability; until then, and for the whole run if that never succeeds, by the
/// conventional one the adapter falls back to when nothing overrides it (`~/.claude`, `~/.codex`,
/// `~/.grok`). Both go through the same expansion and lexical normalisation as `dir`. Symlinks are
/// not followed. `exclude_id` lets an update compare against every *other* account.
pub(crate) fn colliding_account_dir(
    state: &Arc<AppState>,
    agent: Agent,
    dir: &str,
    exclude_id: Option<&str>,
) -> Result<Option<String>> {
    let key = account_dir_key(dir);
    for existing in state.store.list_accounts()? {
        if existing.agent == agent
            && Some(existing.id.as_str()) != exclude_id
            && account_dir_key(&existing.config_dir) == key
        {
            return Ok(Some(existing.name));
        }
    }
    let default_dir = match state.agent_availability_of(agent).default_account_dir {
        Some(resolved) => hostfs::expand(&resolved),
        None => adapter::default_account_dir(agent, &HashMap::new()),
    };
    let default_dir = hostfs::lexically_normalise(&default_dir);
    if account_dir_key(&default_dir.to_string_lossy()) != key {
        return Ok(None);
    }
    Ok(Some(state.store.default_account_name(agent)?))
}

/// Refuses a directory another account of the agent already uses, naming that account.
fn check_account_dir_unique(
    state: &Arc<AppState>,
    agent: Agent,
    dir: &str,
    exclude_id: Option<&str>,
) -> Result<()> {
    if let Some(existing) = colliding_account_dir(state, agent, dir, exclude_id)? {
        return Err(CodedError::raised(
            error_code::CONFIG_DIR_TAKEN,
            format!(
                "the directory `{dir}` is already used by `{existing}` for {}",
                agent.label()
            ),
            &[("agent", agent.label()), ("name", &existing)],
        ));
    }
    Ok(())
}

/// A name as stored: trimmed, and refused if that leaves nothing.
pub(crate) fn normalize_account_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(field_required("name"));
    }
    Ok(trimmed.to_string())
}

pub(super) fn create_account(
    state: &Arc<AppState>,
    agent: Agent,
    name: String,
    config_dir: String,
) -> Result<()> {
    insert_new_account(state, agent, name, config_dir, false)?;
    Ok(())
}

/// Creates an account through the checks of the Add account form (the name rules, the directory
/// rules, a name and a directory no other account of the agent has), stores it with the given
/// `disabled` flag, broadcasts the settings and returns it. Sharing is not set: it is a change of
/// its own (`account_sharing::set`).
pub(crate) fn insert_new_account(
    state: &Arc<AppState>,
    agent: Agent,
    name: String,
    config_dir: String,
    disabled: bool,
) -> Result<Account> {
    let name = normalize_account_name(&name)?;
    let config_dir = normalize_account_dir(agent, &config_dir)?;
    let account = Account {
        id: Uuid::new_v4().to_string(),
        agent,
        name,
        config_dir,
        share_default_config: false,
        share_default_config_incomplete: false,
        disabled,
    };
    {
        // The checks and the insert are one step: another create or update cannot slip between.
        let _writes = state.lock_account_writes();
        check_account_name_unique(state, agent, &account.name, None)?;
        check_account_dir_unique(state, agent, &account.config_dir, None)?;
        state.store.insert_account(&account)?;
    }
    broadcast_settings(state)?;
    Ok(account)
}

fn update_account(
    state: &Arc<AppState>,
    id: &str,
    name: Option<String>,
    config_dir: Option<String>,
) -> Result<()> {
    let repointed = {
        // Reading the account, the checks and the write are one step: another create or update
        // cannot slip between them.
        let _writes = state.lock_account_writes();
        let mut account = state.store.require_account(id)?;
        let mut changed = false;
        let mut dir_changed = false;
        if let Some(name) = name {
            let name = normalize_account_name(&name)?;
            if name != account.name {
                check_account_name_unique(state, account.agent, &name, Some(id))?;
                account.name = name;
                changed = true;
            }
        }
        if let Some(dir) = config_dir {
            let dir = normalize_account_dir(account.agent, &dir)?;
            if dir != account.config_dir {
                check_account_dir_unique(state, account.agent, &dir, Some(id))?;
                account.config_dir = dir;
                changed = true;
                dir_changed = true;
            }
        }
        if !changed {
            return Ok(());
        }
        state.store.update_account(&account)?;
        dir_changed.then_some(account.agent)
    };
    // A reading belongs to the directory it was taken under, and a login signs in to it; a rename
    // leaves both valid.
    if let Some(agent) = repointed {
        state.account_quotas.forget_account(state, id);
        let _ = state.account_logins.cancel(agent, Some(id.to_string()));
        // What runs under the account's directory is now what runs under the new one.
        state.publish_account_sharing();
    }
    broadcast_settings(state)?;
    Ok(())
}

/// Renames or repoints `account` as an `update_account` request asks.
pub(super) async fn update_account_requested(
    state: &Arc<AppState>,
    account: String,
    name: Option<String>,
    config_dir: Option<String>,
) -> Result<()> {
    // A sharing account's directory is brought back to its own before it is let go, once
    // the new directory is known to be acceptable.
    let _sharing = match &config_dir {
        Some(dir) => {
            let agent = state.store.require_account(&account)?.agent;
            let dir = normalize_account_dir(agent, dir)?;
            check_account_dir_unique(state, agent, &dir, Some(&account))?;
            if let Some(name) = &name {
                let name = normalize_account_name(name)?;
                check_account_name_unique(state, agent, &name, Some(&account))?;
            }
            super::account_sharing::before_repoint(state, &account, &dir).await?
        }
        None => None,
    };
    update_account(state, &account, name, config_dir)?;
    Ok(())
}

fn delete_account(state: &Arc<AppState>, id: &str) -> Result<()> {
    let account = state.store.require_account(id)?;
    let affected_consoles: Vec<String> = state
        .store
        .list_consoles()?
        .into_iter()
        .filter(|console| console_references_account(console, id))
        .map(|console| console.id)
        .collect();
    let affected_projects: Vec<String> = state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project_references_account(project, id))
        .map(|project| project.id)
        .collect();
    state.store.delete_account(id)?;
    state.account_quotas.forget_account(state, id);
    state.publish_account_sharing();
    let _ = state
        .account_logins
        .cancel(account.agent, Some(id.to_string()));
    for console_id in affected_consoles {
        if let Some(console) = state.store.get_console(&console_id)? {
            state.broadcast(Event::ConsoleUpserted { console });
        }
    }
    for project_id in affected_projects {
        if let Some(project) = state.store.get_project(&project_id)? {
            state.broadcast(Event::ProjectUpserted { project });
        }
    }
    broadcast_settings(state)?;
    Ok(())
}

/// Deletes `account` as a `delete_account` request asks, with `trash_directory` moving its
/// directory to the Trash first.
pub(super) async fn delete_account_requested(
    state: &Arc<AppState>,
    account: String,
    trash_directory: bool,
) -> Result<()> {
    // Checked before anything is changed, sharing turned off included, so a refusal
    // leaves everything as it was.
    // Once the pre-check has passed, and until the removal ends however it does, no sign-in
    // starts for the account: it would write into the directory after its sign-in was ended.
    let _removing = if trash_directory {
        crate::accounts::trash::directory_to_trash(state, &account).await?;
        state
            .store
            .get_account(&account)?
            .map(|account| state.begin_account_removal(&account))
    } else {
        None
    };
    // A sharing account's directory is brought back to its own before it is let go, so it
    // is never a directory of links into the default one that goes to the Trash.
    let _sharing = super::account_sharing::before_delete(state, &account).await?;
    if trash_directory {
        // The account is locked now, so it cannot be repointed and no session of it starts
        // from here on: what was judged before the lock is judged again.
        // The directory goes first, so a failure keeps the account for a retry.
        crate::accounts::trash::move_directory(state, &account).await?;
    }
    delete_account(state, &account)?;
    Ok(())
}

/// Starts reading the quotas of the accounts the request names, and returns once the readings are
/// started: the account `account` of `agent` when both are given, every account of `agent` when
/// only `agent` is, the account `account` alone when only `account` is, and every account of every
/// available agent when neither is.
pub(super) fn refresh_account_quotas(
    state: &Arc<AppState>,
    agent: Option<Agent>,
    account: Option<String>,
) -> Result<()> {
    let scope = match (agent, account) {
        (Some(agent), account) => {
            checked_account_reference(state, agent, account.clone())?;
            quotas::Scope {
                agent: Some(agent),
                account,
                ..Default::default()
            }
        }
        (None, Some(account)) => {
            let stored = state.store.require_account(&account)?;
            quotas::Scope {
                agent: Some(stored.agent),
                account: Some(account),
                ..Default::default()
            }
        }
        (None, None) => quotas::Scope::default(),
    };
    // The readings take seconds; each is announced as it lands.
    let state = state.clone();
    tokio::spawn(async move { quotas::refresh_requested(&state, &scope).await });
    Ok(())
}

/// Starts a sign-in of the account `account` of `agent`, or of its default account for `None`.
pub(super) async fn start_account_login(
    state: &Arc<AppState>,
    agent: Agent,
    account: Option<String>,
) -> Result<()> {
    refuse_if_agent_unavailable(state, agent)?;
    // Read once: the id and the directory the login runs against are the same row's.
    let stored = match &account {
        Some(id) => Some(
            state
                .store
                .get_account(id)?
                .filter(|stored| stored.agent == agent)
                .ok_or_else(|| CodedError::unknown_account(id))?,
        ),
        None => None,
    };
    let config_dir = stored.map(|stored| stored.config_dir);
    let resolved_dir = config_dir
        .clone()
        .or_else(|| state.agent_availability_of(agent).default_account_dir);
    login::AccountLogins::start(
        state,
        login::Start {
            agent,
            account: account.clone(),
            config_dir: config_dir.clone(),
            resolved_dir,
        },
    )
    .await?;
    // A delete or repoint between the read and the login being registered missed the
    // cancel it makes: that cancel follows its write, so the write is seen here.
    if let Some(id) = &account {
        let current = state.store.get_account(id)?.map(|stored| stored.config_dir);
        if current != config_dir {
            let _ = state.account_logins.cancel(agent, account);
        }
    }
    Ok(())
}

/// Adds an agent that is installed. While availability is still undetermined it is refused rather
/// than guessed at, and the list is filled first if the determination has landed since: the list
/// must be decided by `availability::fill_added_agents` before a user's edit takes it over, or the
/// agents with accounts would be left off it.
pub(super) fn add_agent(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    match state.agent_availability_of(agent).availability {
        Availability::Available => {}
        Availability::Unavailable => {
            return Err(CodedError::raised(
                error_code::AGENT_NOT_INSTALLED,
                format!("{} is not installed on this machine", agent.label()),
                &[("agent", agent.label())],
            ));
        }
        Availability::NotDetermined => {
            return Err(CodedError::raised(
                error_code::AGENT_AVAILABILITY_PENDING,
                format!(
                    "{} cannot be added until it is known which agents are installed",
                    agent.label()
                ),
                &[("agent", agent.label())],
            ));
        }
    }
    availability::fill_added_agents(state)?;
    if !state.store.add_added_agent(agent)? {
        return Err(CodedError::raised(
            error_code::AGENT_ALREADY_ADDED,
            format!("{} is already added", agent.label()),
            &[("agent", agent.label())],
        ));
    }
    broadcast_settings(state)?;
    Ok(())
}

/// Takes an agent off the list. Its accounts stay, so they reappear when it is added again. An
/// undecided list is filled first, as `add_agent` does, so the fill cannot put the agent back; if
/// availability is still unknown it stays undecided and the request is refused.
pub(super) fn remove_agent(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    availability::fill_added_agents(state)?;
    if state.store.added_agents()?.is_none() {
        return Err(added_agents_undecided("removed from"));
    }
    if state.store.remove_added_agent(agent)? {
        broadcast_settings(state)?;
    }
    Ok(())
}

/// Puts the added agents in the given order. An undecided list is filled first, as `add_agent`
/// does; if availability is still unknown it stays undecided and the request is refused.
pub(super) fn reorder_agents(state: &Arc<AppState>, order: &[Agent]) -> Result<()> {
    availability::fill_added_agents(state)?;
    match state.store.reorder_added_agents(order)? {
        ReorderOutcome::Undecided => Err(added_agents_undecided("reordered")),
        ReorderOutcome::NotAPermutation => Err(CodedError::raised(
            error_code::AGENT_ORDER_INVALID,
            "the new order must name each added agent exactly once".to_string(),
            &[],
        )),
        ReorderOutcome::Unchanged => Ok(()),
        ReorderOutcome::Reordered => broadcast_settings(state),
    }
}

/// The refusal of a change to the added agents while the list is still undecided.
fn added_agents_undecided(what: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::ADDED_AGENTS_UNDECIDED,
        format!("the added agents cannot be {what} until it is known which agents are installed"),
        &[],
    )
}

fn console_references_account(console: &Console, account_id: &str) -> bool {
    console
        .accounts
        .iter()
        .any(|(_, referenced)| referenced.as_deref() == Some(account_id))
}

fn project_references_account(project: &Project, account_id: &str) -> bool {
    project
        .accounts
        .iter()
        .any(|(_, referenced)| referenced.account_id() == Some(account_id))
}

#[cfg(test)]
mod tests;
