//! Whether a new session could open on an agent or an account as things stand, and why one cannot:
//! the agent's availability and the account's own state, judged from what the daemon holds.

use std::sync::Arc;

use anyhow::Result;

use super::account_of_agent;
use crate::accounts::quotas;
use crate::protocol::{Agent, Availability};
use crate::state::AppState;
use crate::time::now_millis;

/// Whether `agent` is not known to be missing; while availability is still undetermined it is kept.
pub(crate) fn agent_is_available(state: &Arc<AppState>, agent: Agent) -> bool {
    state.agent_availability_of(agent).availability != Availability::Unavailable
}

/// The used-up window that keeps a new session off `account_id` (`None` the default account) of
/// `agent` at `now`, judged from the reading held alone; `config_dir` is the account's directory,
/// so a reading taken under another one does not count. What
/// [`refuse_if_account_quota_exhausted`] refuses and what [`account_can_open`] passes over.
///
/// [`refuse_if_account_quota_exhausted`]: super::refusals::refuse_if_account_quota_exhausted
pub(super) fn used_up_quota(
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
///
/// [`refuse_if_agent_unavailable`]: super::refusals::refuse_if_agent_unavailable
/// [`refuse_unless_agent_added`]: super::refusals::refuse_unless_agent_added
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
/// [`refuse_if_account_disabled`]: crate::coordinator::accounts::refuse_if_account_disabled
/// [`refuse_if_account_quota_exhausted`]: super::refusals::refuse_if_account_quota_exhausted
/// [`refuse_if_account_not_signed_in`]: super::refusals::refuse_if_account_not_signed_in
/// [`checked_account_reference`]: super::checked_account_reference
pub(crate) fn account_can_open(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<bool> {
    Ok(blocked_by(state, agent, account_id)?.is_none())
}

/// Why a new session cannot run on an account, in the order [`blocked_by`] judges them.
pub(crate) enum CannotOpen {
    /// The account is turned off, or is no longer one of the agent's.
    Unusable,
    /// The last quota reading found it not logged in.
    SignedOut,
    /// Its plan quota is used up.
    QuotaUsedUp(quotas::Exhausted),
    /// A change of its sharing settings is running on it, which a launch would be refused for as
    /// `account_sharing_in_progress`.
    SharingInProgress,
}

/// What keeps a new session of `agent` off `account_id` (`None` the default account) as things
/// stand, or `None` when nothing does: [`blocked_by`], then a sharing change running on the
/// account. For a caller that lists accounts and says why one is passed over, or only decides
/// whether one is.
pub(crate) fn cannot_open(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<Option<CannotOpen>> {
    if let Some(blocked) = blocked_by(state, agent, account_id)? {
        return Ok(Some(blocked));
    }
    Ok(account_id
        .is_some_and(|id| state.sharing_locked(id).is_some())
        .then_some(CannotOpen::SharingInProgress))
}

/// What keeps a new session off `account_id` (`None` the default account) of `agent`, going by
/// what is held now: what [`refuse_if_account_disabled`], [`refuse_if_account_not_signed_in`] and
/// [`refuse_if_account_quota_exhausted`] refuse, the quota judged last. A sharing change running
/// on the account is not counted; see [`cannot_open`].
///
/// [`refuse_if_account_disabled`]: crate::coordinator::accounts::refuse_if_account_disabled
/// [`refuse_if_account_quota_exhausted`]: super::refusals::refuse_if_account_quota_exhausted
/// [`refuse_if_account_not_signed_in`]: super::refusals::refuse_if_account_not_signed_in
pub(super) fn blocked_by(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<Option<CannotOpen>> {
    if state.store.account_disabled(agent, account_id)? {
        return Ok(Some(CannotOpen::Unusable));
    }
    let Some(config_dir) = candidate_dir(state, agent, account_id)? else {
        return Ok(Some(CannotOpen::Unusable));
    };
    if state
        .account_quotas
        .signed_out(agent, account_id, config_dir.as_deref())
    {
        return Ok(Some(CannotOpen::SignedOut));
    }
    Ok(used_up_quota(
        state,
        agent,
        account_id,
        config_dir.as_deref(),
        now_millis(),
    )
    .map(CannotOpen::QuotaUsedUp))
}

/// The directory of the account candidate `account_id` of `agent` (`Some(None)` for the default
/// account, which has none of its own), or `None` when it is no longer one of the agent's
/// accounts.
pub(super) fn candidate_dir(
    state: &Arc<AppState>,
    agent: Agent,
    account_id: Option<&str>,
) -> Result<Option<Option<String>>> {
    Ok(match account_id {
        None => Some(None),
        Some(id) => account_of_agent(state, agent, id)?.map(|account| Some(account.config_dir)),
    })
}
