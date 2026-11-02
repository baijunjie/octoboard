//! Which agent and account a session opens with, and the refusals of an agent or an account that
//! cannot open one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::accounts::quotas;
use crate::adapter;
use crate::protocol::{
    error_code, Account, Agent, Availability, CodedError, Console, Project, ProjectAccount,
};
use crate::state::AppState;
use crate::time::now_millis;

/// Whether `agent` is not known to be missing; while availability is still undetermined it is kept.
pub(crate) fn agent_is_available(state: &Arc<AppState>, agent: Agent) -> bool {
    state.agent_availability_of(agent).availability != Availability::Unavailable
}

/// The directory of an account: the pinned one, else the default account's, resolved from
/// `shell_env`, the snapshot the caller took when it had a default side.
pub(super) fn account_dir(
    agent: Agent,
    pinned: Option<&str>,
    shell_env: Option<&HashMap<String, String>>,
) -> PathBuf {
    match (pinned, shell_env) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, Some(shell_env)) => adapter::default_account_dir(agent, shell_env),
        (None, None) => unreachable!("a default account takes the snapshot"),
    }
}

/// Refuses to open a session whose resolved agent has been *determined* unavailable — never while
/// that determination is still pending, which the launch's own refusal of a missing binary already
/// covers; see "When the agent or the account cannot open a session" in
/// `docs/product/opening-sessions.md`. Checked here, ahead of everything that follows, so this is
/// also what a console session's own `new_session` tool is refused through — it calls this same
/// function.
pub(super) fn refuse_if_agent_unavailable(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    if agent_is_available(state, agent) {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::AGENT_NOT_AVAILABLE,
        format!(
            "{} is not available: its binary does not resolve on your login shell's PATH. {} \
             supports Claude Code, Codex and Grok Build; install one of them and make sure it is \
             on your PATH to open a session.",
            agent.label(),
            crate::APP_NAME
        ),
        &[("agent", agent.label())],
    ))
}

/// Refuses to put a session on an account whose plan quota is used up, going by the reading held
/// at this moment (see [`quotas::AccountQuotas::exhausted`]): an account not read yet, or
/// of an agent that is not probed, is not refused, and nothing waits for a fresh reading. Applies
/// to every origin, the user's own request included. `config_dir` is the account's directory as
/// [`Store::account_dir`] resolves it, so a reading taken under another directory is ignored.
///
/// [`Store::account_dir`]: crate::store::Store::account_dir
pub(super) fn refuse_if_account_quota_exhausted(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> Result<()> {
    let now = now_millis();
    let Some(exhausted) = used_up_quota(state, agent, account_id, config_dir, now) else {
        return Ok(());
    };
    let account = state.store.account_display_name(agent, account_id)?;
    let resets = exhausted.resets_at.map(|at| at.to_string());
    let minutes = exhausted.window_minutes.map(|minutes| minutes.to_string());
    let mut params = vec![
        ("agent", agent.label()),
        ("account", account.as_str()),
        ("window", exhausted.window.as_str()),
        ("window_kind", exhausted.kind.wire_name()),
    ];
    if let Some(minutes) = minutes.as_deref() {
        params.push(("window_minutes", minutes));
    }
    if exhausted.has_model_weekly {
        params.push(("model_weekly", "true"));
    }
    if let Some(resets) = resets.as_deref() {
        params.push(("resets_at", resets));
    }
    let resets_in = exhausted
        .resets_at
        .map(|at| format!(", which resets in about {}", duration_label(at - now)))
        .unwrap_or_default();
    Err(CodedError::raised(
        error_code::ACCOUNT_QUOTA_EXHAUSTED,
        format!(
            "{account} has used up its {} quota ({}){resets_in}. Choose another account, or wait \
             for the quota to reset.",
            agent.label(),
            exhausted.named_for(agent)
        ),
        &params,
    ))
}

/// Refuses an account the held quota reading found not signed in (see
/// [`quotas::AccountQuotas::signed_out`]); a reading that failed for another reason, or
/// none yet, never refuses. `config_dir` is the account's directory as [`Store::account_dir`]
/// resolves it, so a reading taken under another directory is ignored.
///
/// [`Store::account_dir`]: crate::store::Store::account_dir
pub(super) fn refuse_if_account_not_signed_in(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
    config_dir: Option<&str>,
) -> Result<()> {
    if !state
        .account_quotas
        .signed_out(agent, account_id, config_dir)
    {
        return Ok(());
    }
    let account = state.store.account_display_name(agent, account_id)?;
    Err(CodedError::raised(
        error_code::ACCOUNT_NOT_SIGNED_IN,
        format!(
            "{account} ({}) is not logged in. Choose another account, or log in to it in Settings \
             → Agent accounts.",
            agent.label()
        ),
        &[("agent", agent.label()), ("account", account.as_str())],
    ))
}

/// The used-up window that keeps a new session off `account_id` (`None` the default account) of
/// `agent` at `now`, judged from the reading held alone; `config_dir` is the account's directory,
/// so a reading taken under another one does not count. What
/// [`refuse_if_account_quota_exhausted`] refuses and what [`account_can_open`] passes over.
fn used_up_quota(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
    config_dir: Option<&str>,
    now: i64,
) -> Option<quotas::Exhausted> {
    state
        .account_quotas
        .exhausted(agent, account_id, config_dir, now)
}

/// Whether a new session of `agent` would get past the agent's own refusals as things stand
/// ([`refuse_if_agent_unavailable`], [`refuse_unless_agent_added`]). For a caller that skips such
/// an agent or falls back from it, rather than refusing.
pub(crate) fn agent_can_open(state: &Arc<AppState>, agent: Agent) -> Result<bool> {
    Ok(agent_is_available(state, agent) && state.store.agent_is_added(agent)?)
}

/// Whether a new session of `agent` could run on `account_id` (`None` the default account) as
/// things stand: the account is still one of that agent's ([`checked_account_reference`]), it is
/// not turned off ([`refuse_if_account_disabled`]), its quota is not used up
/// ([`refuse_if_account_quota_exhausted`]) and it is not found logged out
/// ([`refuse_if_account_not_signed_in`]). For a caller choosing a default
/// it can fall back from, rather than a refusal.
///
/// [`refuse_if_account_disabled`]: super::accounts::refuse_if_account_disabled
pub(crate) fn account_can_open(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<bool> {
    if state.store.account_disabled(agent, account_id)? {
        return Ok(false);
    }
    let Some(config_dir) = candidate_dir(state, agent, account_id)? else {
        return Ok(false);
    };
    if state
        .account_quotas
        .signed_out(agent, account_id, config_dir.as_deref())
    {
        return Ok(false);
    }
    Ok(used_up_quota(
        state,
        agent,
        account_id,
        config_dir.as_deref(),
        now_millis(),
    )
    .is_none())
}

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
    let usable = |agent, account: Option<&str>| open_now(state, agent, account);
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

/// Whether a new session of `agent` could open on `account` now: [`account_can_open`] and no
/// sharing change running on it (the launch would be refused as `account_sharing_in_progress`).
fn open_now(state: &Arc<AppState>, agent: Agent, account: Option<&str>) -> Result<bool> {
    Ok(
        !account.is_some_and(|id| state.sharing_locked(id).is_some())
            && account_can_open(state, agent, account)?,
    )
}

/// Every account of `agent`, `None` the default account, in the order the user keeps them in
/// (`Store::account_order`), which the session dialog lists them in: where an automatic choice
/// finds candidates alike, the earlier one wins. Accounts the saved order does not name follow, the
/// default account first and then the stored accounts as they are kept. The window's dialogs rank
/// in the same order to preselect what this would pick
/// (`packages/ui/src/accounts/accountChoices.ts`), so a change to the ranking or its order is made
/// on both sides. The MCP tools that list accounts (`list_agents`, `get_settings`) list them in
/// this order too.
pub(crate) fn account_candidates(
    state: &Arc<AppState>,
    agent: Agent,
) -> Result<Vec<Option<String>>> {
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

/// The directory of the account candidate `account_id` of `agent` (`Some(None)` for the default
/// account, which has none of its own), or `None` when it is no longer one of the agent's
/// accounts.
fn candidate_dir(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<Option<Option<String>>> {
    Ok(match account_id {
        None => Some(None),
        Some(id) => account_of_agent(state, agent, id)?.map(|account| Some(account.config_dir)),
    })
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
    let now = now_millis();
    let mut soonest: Option<(i64, Option<String>)> = None;
    for account_id in account_candidates(state, agent)? {
        if open_now(state, agent, account_id.as_deref())? {
            return Ok(None);
        }
        if state.store.account_disabled(agent, account_id.as_deref())? {
            continue;
        }
        if account_id
            .as_deref()
            .is_some_and(|id| state.sharing_locked(id).is_some())
        {
            continue;
        }
        let Some(config_dir) = candidate_dir(state, agent, account_id.as_deref())? else {
            continue;
        };
        if state
            .account_quotas
            .signed_out(agent, account_id.as_deref(), config_dir.as_deref())
        {
            continue;
        }
        let Some(exhausted) = used_up_quota(
            state,
            agent,
            account_id.as_deref(),
            config_dir.as_deref(),
            now,
        ) else {
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
        if !open_now(state, agent, account_id.as_deref())? {
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

/// A span of milliseconds in the rough terms of a message to read: `3d 4h`, `5h 20m` or `7m`.
fn duration_label(millis: i64) -> String {
    let minutes = millis.max(0) / 60_000;
    let (days, hours, minutes) = (minutes / (24 * 60), minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{}m", minutes.max(1)),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}

/// Refuses to open a session of an agent that is not on the added list; see "When the agent or the
/// account cannot open a session" in `docs/product/opening-sessions.md`. An undecided list refuses
/// nothing. Only a session being opened is checked, so a live session of an agent removed since
/// keeps running, and resuming, restarting or reopening an existing session never comes here.
pub(super) fn refuse_unless_agent_added(state: &Arc<AppState>, agent: Agent) -> Result<()> {
    if state.store.agent_is_added(agent)? {
        return Ok(());
    }
    Err(CodedError::raised(
        error_code::AGENT_NOT_ADDED,
        format!(
            "{} is not added. Add it in Settings → Agent accounts to open a session with it.",
            agent.label()
        ),
        &[("agent", agent.label())],
    ))
}

/// The agent a session is of: an explicit choice wins over the project's default, then the
/// console's project session default. A console session has no project, so it falls straight
/// back to the console's console session default.
pub(crate) fn session_agent(
    chosen: Option<Agent>,
    project: Option<&Project>,
    console: &Console,
) -> Agent {
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
/// [`open_session`]: super::open::open_session
pub(super) fn session_account(
    state: &Arc<AppState>,
    agent: Agent,
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
pub(super) fn checked_account_reference(
    state: &Arc<AppState>,
    agent: Agent,
    id: Option<String>,
) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    match account_of_agent(state, agent, &id)? {
        Some(_) => Ok(Some(id)),
        None => Err(CodedError::unknown_account(&id)),
    }
}

/// The stored account `id`, when it is one of `agent`'s.
pub(super) fn account_of_agent(
    state: &Arc<AppState>,
    agent: Agent,
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
pub(super) fn checked_project_account(
    state: &Arc<AppState>,
    agent: Agent,
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

#[cfg(test)]
mod tests;
