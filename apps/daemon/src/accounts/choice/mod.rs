//! Which agent and account a session opens with, and what keeps an agent or an account from
//! opening one: resolving a session's agent and account (this file), the refusals (`refusals`),
//! whether an agent or account can open a session now (`standing`), automatic selection and the
//! order of an agent's accounts (`automatic`) and the choice for a session started by another
//! (`started`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::adapter;
use crate::protocol::{Account, AgentId, CodedError, Console, Project, ProjectAccount};
use crate::state::AppState;

mod automatic;
mod refusals;
mod standing;
mod started;

pub(crate) use automatic::{
    automatic_open_choice, candidate_accounts, preferred_weekly_account_except,
};
#[cfg(test)]
pub(crate) use automatic::{preferred_weekly_account, soonest_available_account};
pub(crate) use refusals::{
    refuse_if_account_not_signed_in, refuse_if_account_quota_exhausted,
    refuse_if_agent_unavailable, refuse_unless_agent_added,
};
pub(crate) use standing::{
    account_can_open, agent_can_open, agent_is_available, cannot_open, CannotOpen,
};
pub(crate) use started::choice_for_started_session;

/// The directory of an account: the pinned one, else the default account's, resolved from
/// `shell_env`, the snapshot the caller took when it had a default side.
pub(crate) fn account_dir(
    agent: AgentId,
    pinned: Option<&str>,
    shell_env: Option<&HashMap<String, String>>,
) -> PathBuf {
    match (pinned, shell_env) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, Some(shell_env)) => adapter::default_account_dir(agent, shell_env),
        (None, None) => unreachable!("a default account takes the snapshot"),
    }
}

/// The agent a session is of: an explicit choice wins over the project's default, then the
/// console's project session default. A console session has no project, so it falls straight
/// back to the console's console session default.
pub(crate) fn session_agent(
    chosen: Option<AgentId>,
    project: Option<&Project>,
    console: &Console,
) -> AgentId {
    chosen.unwrap_or_else(|| match project {
        Some(project) => project
            .default_agent
            .unwrap_or_else(|| console.project_session_default()),
        None => console.default_agent,
    })
}

/// The account id and the config directory a session of `agent` should launch with, and that
/// account's directory at this moment — `None` for both when it is the default account (the state
/// of pinning nothing). In descending priority: the choice made for this session (`chosen`, where
/// `Some(None)` is a choice of the default account), the project's account for the agent, the
/// console's account for the agent, the agent's default account. A project that says the default
/// account ends the chain there, whatever the console references; one that inherits falls through
/// to the console. Mirrors the agent chain in
/// [`open_session`]. A console session has no project, so its chain skips that step.
///
/// [`open_session`]: crate::coordinator::open::open_session
pub(crate) fn session_account(
    state: &Arc<AppState>,
    agent: AgentId,
    console: &Console,
    project: Option<&Project>,
    chosen: Option<Option<String>>,
) -> Result<(Option<String>, Option<String>)> {
    let account_id = match chosen {
        Some(choice) => checked_account_reference(state, agent, choice)?,
        None => match project {
            Some(project) => console.resolve_project_account(agent, project.accounts.get(agent)),
            None => console.accounts.get(agent).clone(),
        },
    };
    let config_dir = state.store.account_dir(&account_id)?;
    Ok((account_id, config_dir))
}

/// A reference to an account of `agent` as stored, or the `unknown_account` refusal when `id`
/// names no account or one of another agent: an account belongs to one agent, and a reference
/// across agents would launch it with another agent's directory.
pub(crate) fn checked_account_reference(
    state: &Arc<AppState>,
    agent: AgentId,
    id: Option<String>,
) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    match account_of_agent(state, agent, &id)? {
        Some(_) => Ok(Some(id)),
        None => Err(CodedError::unknown_account(&id)),
    }
}

/// The stored account `id`, when it is one of `agent`'s.
pub(crate) fn account_of_agent(
    state: &Arc<AppState>,
    agent: AgentId,
    id: &str,
) -> Result<Option<Account>> {
    Ok(state
        .store
        .get_account(id)?
        .filter(|account| account.agent == agent))
}

/// A project's setting for `agent` as stored, or the `unknown_account` refusal when it names an
/// account that does not exist or belongs to another agent; the other two states name nothing to
/// check.
pub(crate) fn checked_project_account(
    state: &Arc<AppState>,
    agent: AgentId,
    account: ProjectAccount,
) -> Result<ProjectAccount> {
    match account {
        ProjectAccount::Account { id } => {
            checked_account_reference(state, agent, Some(id.clone()))?;
            Ok(ProjectAccount::Account { id })
        }
        other => Ok(other),
    }
}
