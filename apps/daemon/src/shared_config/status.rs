//! Each owned account's sharing state as clients are told it (`account_sharing`): what runs under
//! its directory, the change running and the last one that failed. What runs there is read off the
//! sessions and the agent CLI runs whenever one of them may have changed; the change's progress
//! and failure are kept here. A state is sent only when it differs from the one last sent.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;

use super::in_use;
use crate::protocol::{
    AccountInUse, AccountSharing, Event, Session, SharingFailure, SharingProgress,
};
use crate::state::AppState;

/// The accounts' sharing states, by account id. The lock is taken for the whole of a
/// publication, so states are sent in the order they were read.
#[derive(Default)]
pub struct States(Mutex<HashMap<String, Entry>>);

/// Whether each session of an account with a process was last seen working (`true`) or idle, so a
/// change that leaves it as it was publishes nothing.
#[derive(Default)]
pub struct Activity(Mutex<HashMap<String, bool>>);

/// Publishes the sharing states when what was just published of `session`, its stored record,
/// moved it between working and idle, or its process came or went. Cheap otherwise: a session's
/// record is published on every change of its title or status.
pub fn note_session(state: &AppState, session: &Session) {
    if session.account_id.is_none() && session.config_dir.is_none() {
        return;
    }
    let working = state.activity_of(session);
    let changed = {
        let mut seen = state
            .account_sharing
            .activity
            .0
            .lock()
            .expect("account activity lock poisoned");
        match working {
            Some(working) => seen.insert(session.id.clone(), working) != Some(working),
            None => seen.remove(&session.id).is_some(),
        }
    };
    if changed {
        publish(state);
    }
}

/// Drops what [`note_session`] last saw of a session whose record is gone.
pub fn forget_session(state: &AppState, id: &str) {
    state
        .account_sharing
        .activity
        .0
        .lock()
        .expect("account activity lock poisoned")
        .remove(id);
}

#[derive(Default)]
struct Entry {
    progress: Option<SharingProgress>,
    failure: Option<SharingFailure>,
    sent: Option<AccountSharing>,
}

/// Sends the sharing state of every owned account that changed since it was last sent. Called
/// wherever what runs under an account's directory may have changed: a session's claim taken or
/// let go, its process ended, its account switched, a sign-in started or ended, an account
/// repointed or removed.
pub fn publish(state: &AppState) {
    update(state, |_| {});
}

/// Records the progress of `account`'s change (`None` once it has ended) and publishes.
pub(super) fn set_progress(state: &AppState, account: &str, progress: Option<SharingProgress>) {
    update(state, |entries| {
        entries.entry(account.to_string()).or_default().progress = progress;
    });
}

/// Ends `account`'s change, recording `failure` when it failed, and publishes.
pub(super) fn end_change(state: &AppState, account: &str, failure: Option<SharingFailure>) {
    update(state, |entries| {
        let entry = entries.entry(account.to_string()).or_default();
        entry.progress = None;
        entry.failure = failure;
    });
}

/// Starts `account`'s change: the last failure is forgotten and `progress` recorded.
pub(super) fn start_change(state: &AppState, account: &str, progress: SharingProgress) {
    update(state, |entries| {
        let entry = entries.entry(account.to_string()).or_default();
        entry.progress = Some(progress);
        entry.failure = None;
    });
}

/// Every owned account's sharing state now, for a `snapshot`.
pub fn states(state: &AppState) -> Result<Vec<AccountSharing>> {
    let entries = state
        .account_sharing
        .states
        .0
        .lock()
        .expect("account sharing lock poisoned");
    current(state, &entries)
}

fn update(state: &AppState, change: impl FnOnce(&mut HashMap<String, Entry>)) {
    let mut entries = state
        .account_sharing
        .states
        .0
        .lock()
        .expect("account sharing lock poisoned");
    change(&mut entries);
    let states = match current(state, &entries) {
        Ok(states) => states,
        Err(err) => {
            tracing::warn!(%err, "reading the accounts' sharing state failed");
            return;
        }
    };
    entries.retain(|id, _| states.iter().any(|sharing| sharing.account == *id));
    for sharing in states {
        let entry = entries.entry(sharing.account.clone()).or_default();
        if entry.sent.as_ref() == Some(&sharing) {
            continue;
        }
        // A client takes an account it has been told nothing of as free, which is what a state
        // with nothing in it says.
        let empty = sharing.in_use.is_none()
            && sharing.idle_sessions == 0
            && sharing.progress.is_none()
            && sharing.failure.is_none();
        if entry.sent.is_some() || !empty {
            state.broadcast(Event::AccountSharing(sharing.clone()));
        }
        entry.sent = Some(sharing);
    }
}

fn current(state: &AppState, entries: &HashMap<String, Entry>) -> Result<Vec<AccountSharing>> {
    let accounts = state.store.list_accounts()?;
    if accounts.is_empty() {
        return Ok(Vec::new());
    }
    let running = state.running_sessions()?;
    Ok(accounts
        .iter()
        .map(|account| {
            let uses = in_use(account, &running);
            let entry = entries.get(&account.id);
            AccountSharing {
                account: account.id.clone(),
                in_use: (uses.working > 0 || uses.signing_in).then_some(AccountInUse {
                    sessions: uses.working,
                    signing_in: uses.signing_in,
                }),
                idle_sessions: uses.idle.len(),
                progress: entry.and_then(|entry| entry.progress.clone()),
                failure: entry.and_then(|entry| entry.failure.clone()),
            }
        })
        .collect())
}
