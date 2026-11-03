//! Which sessions are bound to which, read from the session rows: an owner's bound sessions, its
//! team, a group, and how many sessions each owner has. The writes, binding, transferring and
//! releasing, are the coordinator's.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;

use super::AppState;
use crate::protocol::Session;

/// The sessions of `sessions` bound to `owner`, in any status. A session with the role of a bound
/// one owns none, so it has none. Walks that build an owner's team or group go through it, so
/// they agree on what is bound to what.
fn bound_in<'a>(sessions: &'a [Session], owner: &Session) -> impl Iterator<Item = &'a Session> {
    let owns = !owner.is_worker();
    let owner_id = owner.id.clone();
    sessions
        .iter()
        .filter(move |session| owns && session.bound_to.as_deref() == Some(owner_id.as_str()))
}

/// `root` and every session below it along the bindings, in any status: the sessions bound to it,
/// each followed by its own. Bindings are two levels deep at most (a console session, its lead
/// sessions, what is bound to those), so the walk goes no further.
pub(crate) fn group_in<'a>(sessions: &'a [Session], root: &'a Session) -> Vec<&'a Session> {
    let mut group = vec![root];
    for bound in bound_in(sessions, root) {
        group.push(bound);
        group.extend(bound_in(sessions, bound));
    }
    group
}

/// How many sessions are bound to each owner of `sessions`, in any status, by the owner's id,
/// counted as [`bound_in`] does: a session with the role of a worker owns none, whatever rows
/// say they are bound to it.
pub(crate) fn owned_counts(sessions: &[Session]) -> HashMap<String, usize> {
    let owns_none: HashSet<&str> = sessions
        .iter()
        .filter(|session| session.is_worker())
        .map(|session| session.id.as_str())
        .collect();
    let mut counts = HashMap::new();
    for owner in sessions
        .iter()
        .filter_map(|session| session.bound_to.as_deref())
    {
        if !owns_none.contains(owner) {
            *counts.entry(owner.to_string()).or_default() += 1;
        }
    }
    counts
}

/// [`owned_counts`] for the one session `owner`, without reading the table of sessions: its
/// entry, or none when it owns none. Counted as [`bound_in`] does, so a session with the role of a
/// worker owns none.
pub(crate) fn owned_count_of(
    state: &Arc<AppState>,
    owner: &Session,
) -> Result<HashMap<String, usize>> {
    let mut counts = HashMap::new();
    if !owner.is_worker() {
        let owned = state.store.count_bound_to(&owner.id)?;
        if owned > 0 {
            counts.insert(owner.id.clone(), owned);
        }
    }
    Ok(counts)
}

/// The sessions bound to `owner`, in any status.
pub(crate) fn bound_sessions(state: &Arc<AppState>, owner: &Session) -> Result<Vec<Session>> {
    let sessions = state.store.list_sessions()?;
    Ok(bound_in(&sessions, owner).cloned().collect())
}

/// Every session below `owner` along the bindings, in any status, the deepest level first: the
/// sessions bound to its lead sessions, then the sessions bound to it.
pub(crate) fn team(state: &Arc<AppState>, owner: &Session) -> Result<Vec<Session>> {
    let sessions = state.store.list_sessions()?;
    let mut team = Vec::new();
    for bound in bound_in(&sessions, owner) {
        team.extend(bound_in(&sessions, bound).cloned());
    }
    team.extend(bound_in(&sessions, owner).cloned());
    Ok(team)
}

#[cfg(test)]
mod tests;
