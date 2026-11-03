//! Choosing a new session's agent and account while automatic selection is on, and the accounts
//! of an agent in the order it weighs them.

use std::sync::Arc;

use anyhow::Result;

use super::session_account;
use super::standing::{agent_can_open, blocked_by, candidate_dir, cannot_open, CannotOpen};
use crate::accounts::quotas;
use crate::protocol::{Agent, Console, Project};
use crate::state::AppState;
use crate::time::now_millis;

/// Chooses a new session's agent and account while automatic selection is on. Each agent
/// keeps its own account preference before another agent is tried. An explicit agent restricts
/// the search to it; an explicit account never comes through this function.
pub(crate) fn automatic_open_choice(
    state: &Arc<AppState>,
    preferred: Agent,
    console: &Console,
    project: Option<&Project>,
    inherited: Option<(Agent, Option<String>)>,
    allow_other_agents: bool,
) -> Result<Option<(Agent, Option<String>)>> {
    let usable = |agent, account: Option<&str>| -> Result<bool> {
        Ok(cannot_open(state, agent, account)?.is_none())
    };
    let mut agents = vec![preferred];
    if allow_other_agents {
        for agent in state
            .store
            .added_agents()?
            .unwrap_or_else(|| Agent::ALL.to_vec())
        {
            if agent != preferred {
                agents.push(agent);
            }
        }
    }
    for agent in agents {
        if !agent_can_open(state, agent)? {
            continue;
        }
        if let Some(account) = preferred_weekly_account(state, agent)? {
            return Ok(Some((agent, account)));
        }
        let inherited = match inherited.as_ref() {
            Some((inherited_agent, account))
                if *inherited_agent == agent && usable(agent, account.as_deref())? =>
            {
                Some(account.clone())
            }
            _ => None,
        };
        let (account, _) = session_account(state, agent, console, project, inherited)?;
        if usable(agent, account.as_deref())? {
            return Ok(Some((agent, account)));
        }
        for account in account_candidates(state, agent)? {
            if usable(agent, account.as_deref())? {
                return Ok(Some((agent, account)));
            }
        }
    }
    // No agent has a usable account: the preferred agent decides, as in the session dialog.
    if agent_can_open(state, preferred)? {
        if let Some(account) = soonest_available_account(state, preferred)? {
            return Ok(Some((preferred, account)));
        }
    }
    Ok(None)
}

/// Every account of `agent`, `None` the default account, in the order the user keeps them in
/// (`Store::account_order`), which the session dialog lists them in: where an automatic choice
/// finds candidates alike, the earlier one wins. Accounts the saved order does not name follow, the
/// default account first and then the stored accounts as they are kept. The window's dialogs rank
/// in the same order to preselect what this would pick
/// (`packages/ui/src/accounts/accountRanking.ts` and `openingChoice.ts`), so a change to the
/// ranking or its order is made on both sides.
fn account_candidates(state: &Arc<AppState>, agent: Agent) -> Result<Vec<Option<String>>> {
    let mut candidates = vec![None];
    for account in state.store.list_accounts()? {
        if account.agent == agent {
            candidates.push(Some(account.id));
        }
    }
    let order = state.store.account_order(agent)?;
    candidates.sort_by_key(|candidate| {
        order
            .iter()
            .position(|id| id == candidate)
            .unwrap_or(usize::MAX)
    });
    Ok(candidates)
}

/// An account of an agent as a listing shows it.
pub(crate) struct Candidate {
    /// `None` for the default account.
    pub(crate) id: Option<String>,
    /// The account's directory; `None` for the default account, which has none of its own.
    pub(crate) config_dir: Option<String>,
    pub(crate) name: String,
    pub(crate) disabled: bool,
}

/// The accounts of `agent` in the order of [`account_candidates`], each as a listing shows it: the
/// default account under the name and the on/off setting the user gave it.
pub(crate) fn candidate_accounts(state: &Arc<AppState>, agent: Agent) -> Result<Vec<Candidate>> {
    let stored = state.store.list_accounts()?;
    let mut listed = Vec::new();
    for id in account_candidates(state, agent)? {
        listed.push(match id {
            None => Candidate {
                id: None,
                config_dir: None,
                name: state.store.default_account_name(agent)?,
                disabled: state.store.account_disabled(agent, None)?,
            },
            Some(id) => {
                let Some(account) = stored
                    .iter()
                    .find(|account| account.agent == agent && account.id == id)
                else {
                    continue;
                };
                Candidate {
                    id: Some(id),
                    config_dir: Some(account.config_dir.clone()),
                    name: account.name.clone(),
                    disabled: account.disabled,
                }
            }
        });
    }
    Ok(listed)
}

/// The account of `agent` that becomes usable soonest, for a new session that leaves its account
/// out while automatic selection is on and no account of the agent can be used. `Some(None)` is
/// the default account. Only an account kept off by a used-up quota alone qualifies (not one turned
/// off, logged out or with a sharing change running); it is usable when the window that blocks it
/// resets, and a blocking window with no reset time counts as never. The earlier candidate in
/// [`account_candidates`] keeps a tie. `None` when an account of the agent is usable, or none
/// qualifies.
pub(crate) fn soonest_available_account(
    state: &Arc<AppState>,
    agent: Agent,
) -> Result<Option<Option<String>>> {
    let mut soonest: Option<(i64, Option<String>)> = None;
    for account_id in account_candidates(state, agent)? {
        if cannot_open(state, agent, account_id.as_deref())?.is_none() {
            return Ok(None);
        }
        if account_id
            .as_deref()
            .is_some_and(|id| state.sharing_locked(id).is_some())
        {
            continue;
        }
        let Some(CannotOpen::QuotaUsedUp(exhausted)) =
            blocked_by(state, agent, account_id.as_deref())?
        else {
            continue;
        };
        let at = exhausted.resets_at.unwrap_or(i64::MAX);
        if soonest.as_ref().is_none_or(|(best, _)| at < *best) {
            soonest = Some((at, account_id));
        }
    }
    Ok(soonest.map(|(_, account)| account))
}

/// The account of `agent` to use when a new session leaves its account out and
/// `auto_select_account` is on. `Some(Some(id))` is a stored account, `Some(None)` the default
/// account, and `None` when no account has a weekly window that still has quota and resets
/// ahead, skipping a disabled account, one found logged out, and an account whose sharing change
/// is running (a launch on it would be refused as `account_sharing_in_progress`). The soonest
/// weekly reset wins, except that weekly resets less than one session window apart from the
/// soonest count as tied. Among the tied, an account whose 5-hour session window is running goes
/// first, then the sooner session reset, the sooner weekly reset and the larger remainder, in
/// that order (see [`quotas::preferred_signal`]); the earlier candidate in
/// [`account_candidates`] wins one that is still tied. Rank
/// from the reading held for the account's current directory. The list of readings carries no
/// directory, so one still stored under another directory would be chosen.
pub(crate) fn preferred_weekly_account(
    state: &Arc<AppState>,
    agent: Agent,
) -> Result<Option<Option<String>>> {
    preferred_weekly_account_except(state, agent, &[], false)
}

/// [`preferred_weekly_account`] without the accounts in `except` (`None` the default account), for
/// a session that is to be moved off one of them. With `readable_only` also without an account
/// whose last attempt to read its quota failed, whatever the reading still holds: a session is
/// not moved onto an account that cannot be seen to have quota left.
pub(crate) fn preferred_weekly_account_except(
    state: &Arc<AppState>,
    agent: Agent,
    except: &[Option<String>],
    readable_only: bool,
) -> Result<Option<Option<String>>> {
    let mut candidates = account_candidates(state, agent)?;
    candidates.retain(|candidate| !except.contains(candidate));
    let now = now_millis();
    let mut signals = Vec::new();
    let mut accounts = Vec::new();
    for account_id in candidates {
        if cannot_open(state, agent, account_id.as_deref())?.is_some() {
            continue;
        }
        let Some(config_dir) = candidate_dir(state, agent, account_id.as_deref())? else {
            continue;
        };
        let Some(quota) =
            state
                .account_quotas
                .held(agent, account_id.as_deref(), config_dir.as_deref())
        else {
            continue;
        };
        if readable_only && quota.unavailable.is_some() {
            continue;
        }
        let Some(signal) = quotas::AccountSignal::of(&quota.windows, now) else {
            continue;
        };
        signals.push(signal);
        accounts.push(account_id);
    }
    Ok(quotas::preferred_signal(&signals).map(|index| accounts.swap_remove(index)))
}
