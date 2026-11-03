//! Each owned account's sharing state as clients are told it (`account_sharing`): what runs under
//! its directory, the change of its sharing running and the last one that failed. What runs there
//! is read off the sessions and the agent CLI runs whenever one of them may have changed; the
//! change's progress and failure are kept here. A state is sent only when it differs from the one
//! last sent.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;

use super::AppState;
use crate::accounts::shared_config::in_use;
use crate::protocol::{
    AccountInUse, AccountSharing, Event, Session, SharingFailure, SharingProgress,
};

/// What [`AppState`] holds of the accounts' sharing states.
#[derive(Default)]
pub struct SharingStatus {
    /// The states by account id. The lock is taken for the whole of a publication, so states are
    /// sent in the order they were read.
    states: Mutex<HashMap<String, Entry>>,
    /// Whether each session of an account with a process was last seen working (`true`) or idle,
    /// so a change that leaves it as it was publishes nothing.
    activity: Mutex<HashMap<String, bool>>,
}

#[derive(Default)]
struct Entry {
    progress: Option<SharingProgress>,
    failure: Option<SharingFailure>,
    sent: Option<AccountSharing>,
}

impl AppState {
    /// Tells the session's account a message queued, written or left unsent may have changed
    /// whether the session is working.
    pub(crate) fn note_activity(&self, id: &str) {
        if let Ok(Some(session)) = self.store.get_session(id) {
            self.note_sharing_activity(&session);
        }
    }

    /// Publishes the sharing states when what was just published of `session`, its stored
    /// record, moved it between working and idle, or its process came or went. Cheap otherwise: a
    /// session's record is published on every change of its title or status.
    pub(super) fn note_sharing_activity(&self, session: &Session) {
        if session.account_id.is_none() && session.config_dir.is_none() {
            return;
        }
        let working = self.activity_of(session);
        let changed = {
            let mut seen = self
                .sharing_status
                .activity
                .lock()
                .expect("account activity lock poisoned");
            match working {
                Some(working) => seen.insert(session.id.clone(), working) != Some(working),
                None => seen.remove(&session.id).is_some(),
            }
        };
        if changed {
            self.publish_account_sharing();
        }
    }

    /// Drops what [`Self::note_sharing_activity`] last saw of a session whose record is gone.
    pub(super) fn forget_sharing_activity(&self, id: &str) {
        self.sharing_status
            .activity
            .lock()
            .expect("account activity lock poisoned")
            .remove(id);
    }

    /// Sends the sharing state of every owned account that changed since it was last sent. Called
    /// wherever what runs under an account's directory may have changed: a session's claim taken
    /// or let go, its process ended, its account switched, a sign-in started or ended, an account
    /// repointed or removed.
    pub fn publish_account_sharing(&self) {
        self.update_sharing(|_| {});
    }

    /// Records the progress of `account`'s change of sharing (`None` once it has ended) and
    /// publishes.
    pub(crate) fn set_sharing_progress(&self, account: &str, progress: Option<SharingProgress>) {
        self.update_sharing(|entries| {
            entries.entry(account.to_string()).or_default().progress = progress;
        });
    }

    /// Starts `account`'s change of sharing: the last failure is forgotten and `progress`
    /// recorded.
    pub(crate) fn start_sharing_change(&self, account: &str, progress: SharingProgress) {
        self.update_sharing(|entries| {
            let entry = entries.entry(account.to_string()).or_default();
            entry.progress = Some(progress);
            entry.failure = None;
        });
    }

    /// Ends `account`'s change of sharing, recording `failure` when it failed, and publishes.
    pub(crate) fn end_sharing_change(&self, account: &str, failure: Option<SharingFailure>) {
        self.update_sharing(|entries| {
            let entry = entries.entry(account.to_string()).or_default();
            entry.progress = None;
            entry.failure = failure;
        });
    }

    /// Every owned account's sharing state now, for a `snapshot`.
    pub fn account_sharing_states(&self) -> Result<Vec<AccountSharing>> {
        let entries = self
            .sharing_status
            .states
            .lock()
            .expect("account sharing lock poisoned");
        self.current_sharing(&entries)
    }

    fn update_sharing(&self, change: impl FnOnce(&mut HashMap<String, Entry>)) {
        let mut entries = self
            .sharing_status
            .states
            .lock()
            .expect("account sharing lock poisoned");
        change(&mut entries);
        let states = match self.current_sharing(&entries) {
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
            // A client takes an account it has been told nothing of as free, which is what a
            // state with nothing in it says.
            let empty = sharing.in_use.is_none()
                && sharing.idle_sessions == 0
                && sharing.progress.is_none()
                && sharing.failure.is_none();
            if entry.sent.is_some() || !empty {
                self.broadcast(Event::AccountSharing(sharing.clone()));
            }
            entry.sent = Some(sharing);
        }
    }

    fn current_sharing(&self, entries: &HashMap<String, Entry>) -> Result<Vec<AccountSharing>> {
        let accounts = self.store.list_accounts()?;
        if accounts.is_empty() {
            return Ok(Vec::new());
        }
        let running = self.running_sessions()?;
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
}
