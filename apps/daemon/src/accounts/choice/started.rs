//! The agent and account of a session one session starts for another.

use std::sync::Arc;

use anyhow::Result;

use super::automatic::automatic_open_choice;
use super::session_agent;
use super::standing::{account_can_open, agent_can_open};
use crate::protocol::{Agent, Console, Project, Role, Session};
use crate::state::AppState;

/// The agent and the account of a session `caller` starts in `project` of `console` (none for a
/// console session), for the agent the call names (`chosen_agent`) or none. `named_account` reads
/// the account the call names, for the agent the session will be of, which is settled first, as
/// `Some(account)` (`Some(None)` the default account) or `None` when the call names none. Returns
/// the agent to open with, `None` leaving it to the project's default agent, else the console's,
/// and the account, `None` leaving it to the project's account for the agent, else the console's.
pub(crate) fn choice_for_started_session(
    state: &Arc<AppState>,
    caller: &Session,
    console: &Console,
    project: Option<&Project>,
    chosen_agent: Option<Agent>,
    named_account: impl FnOnce(Agent) -> Result<Option<Option<String>>>,
) -> Result<(Option<Agent>, Option<Option<String>>)> {
    // Only in the caller's own console does a session follow the caller's agent and account:
    // another console is set up apart, and its sessions run on what it and its projects set. A
    // console session does not follow its own agent into a project either: the project's default
    // agent is what its sessions run on, unless the call names one.
    let in_own_console = caller.console_id == console.id;
    let follows_callers_agent =
        in_own_console && !(caller.role == Role::Console && project.is_some());
    // Read once: the agent and the account are both settled on it.
    let automatic = state.store.auto_select_account()?;
    let mut agent = match chosen_agent {
        Some(agent) => Some(agent),
        None if follows_callers_agent && (automatic || agent_can_open(state, caller.agent)?) => {
            Some(caller.agent)
        }
        None => None,
    };
    // An account is one agent's, so it is read for the agent the session will be of.
    let session_agent = session_agent(agent, project, console);
    let account = match named_account(session_agent)? {
        Some(account) => Some(account),
        None if automatic => {
            match automatic_choice(
                state,
                caller,
                session_agent,
                console,
                project,
                chosen_agent.is_none(),
            )? {
                Some((picked_agent, picked_account)) => {
                    agent = Some(picked_agent);
                    Some(picked_account)
                }
                None => None,
            }
        }
        None if in_own_console => callers_account(state, caller, session_agent)?,
        // Inherit the project, else the console.
        None => None,
    };
    Ok((agent, account))
}

/// The agent and account of a session `caller` starts, or hands one off to, in `project` of
/// `console` (none for a console session), when the call names no account and automatic account
/// selection is on: what [`automatic_open_choice`] picks, as for a session the user opens without
/// choosing one, of `agent` alone unless `other_agents`. `caller`'s own account
/// ([`callers_account`]) is what the session inherits in `caller`'s own console. `None` when
/// nothing is picked: the session then takes the project's account for its agent, else the
/// console's.
fn automatic_choice(
    state: &Arc<AppState>,
    caller: &Session,
    agent: Agent,
    console: &Console,
    project: Option<&Project>,
    other_agents: bool,
) -> Result<Option<(Agent, Option<String>)>> {
    let inherited = if caller.console_id == console.id {
        callers_account(state, caller, caller.agent)?.map(|account| (caller.agent, account))
    } else {
        None
    };
    automatic_open_choice(state, agent, console, project, inherited, other_agents)
}

/// The account a session of `agent` that `caller` starts runs on when the call names none:
/// `caller`'s own, for a session of `caller`'s own agent, so that work a session hands out goes on
/// in the same agent and account unless the user asked otherwise. `None` — the project's account
/// for the agent, else the console's — for another agent, and when `caller`'s account could not
/// open a session now: removed since, turned off, logged out, or with its quota used up.
fn callers_account(
    state: &Arc<AppState>,
    caller: &Session,
    agent: Agent,
) -> Result<Option<Option<String>>> {
    if agent != caller.agent || !account_can_open(state, agent, caller.account_id.as_deref())? {
        return Ok(None);
    }
    Ok(Some(caller.account_id.clone()))
}
