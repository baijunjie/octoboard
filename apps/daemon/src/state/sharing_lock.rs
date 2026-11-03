//! The locks an account's change of sharing takes over its sessions, and the claim that a
//! `delete_account` moving the directory to the Trash holds on the account.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;

use super::claims::LaunchClaim;
use super::live::{LiveSessions, Running};
use super::AppState;
use crate::messaging::outbox::Ticket;
use crate::messaging::write_queue::OutboxHold;
use crate::protocol::Session;

pub(super) struct SharingLockEntry {
    /// The account as it was when locked.
    pub(super) account: crate::protocol::Account,
    /// The sessions the change restarts, until each is resumed or let go.
    pub(super) restarting: HashSet<String>,
}

/// An account marked as being removed (see [`AppState::begin_account_removal`]), until dropped.
pub struct AccountRemoval {
    state: Arc<AppState>,
    account: String,
}

impl Drop for AccountRemoval {
    fn drop(&mut self) {
        let mut removing = self
            .state
            .accounts_being_removed
            .lock()
            .expect("accounts being removed lock poisoned");
        if let Some((_, count)) = removing.get_mut(&self.account) {
            *count -= 1;
            if *count == 0 {
                removing.remove(&self.account);
            }
        }
    }
}

/// The account locked for a change of its sharing that `session` runs under, if any.
pub(super) fn locked_over<'a>(
    live: &'a LiveSessions,
    session: &Session,
) -> Option<&'a SharingLockEntry> {
    live.sharing_locks
        .values()
        .find(|entry| crate::accounts::shared_config::runs_under(&entry.account, session))
}

/// An account locked for a change of its sharing (see [`AppState::lock_account_sharing`]), until
/// dropped, with the claims on the idle sessions the change ends and resumes: nothing else can
/// launch, resume, switch or delete them meanwhile, and a client is shown them `starting` rather
/// than interrupted while their process is down (see [`AppState::shown_status`]).
pub struct AccountSharingLock {
    state: Arc<AppState>,
    account: String,
    claims: HashMap<String, LaunchClaim>,
    /// The claimed sessions' queues, held from the claim until the session is let go: a message
    /// sent to one meanwhile is not written into a process about to be ended, nor forgotten when
    /// it ends, and is left for the resumed process (see [`Self::release_queue_for_relaunch`]), or
    /// dropped with a notice for a session not resumed (see [`OutboxHold`]).
    holds: HashMap<String, OutboxHold>,
}

impl AccountSharingLock {
    /// Claims the sessions `choose` names, idle ones of the account, for the change to restart, in
    /// one step with `choose`'s look at the sessions running or holding a claim, which refuses by
    /// answering an error.
    pub fn claim_idle(
        &mut self,
        choose: impl FnOnce(&[Running]) -> Result<Vec<String>>,
    ) -> Result<()> {
        let mut live = self
            .state
            .live
            .write()
            .expect("live sessions lock poisoned");
        let running = self.state.running_of(&live)?;
        let ids = choose(&running)?;
        let mut claimed = HashSet::new();
        for id in ids {
            // Not claimed already: a claimed session is never idle.
            if live.launching.insert(id.clone()) {
                claimed.insert(id.clone());
                self.holds
                    .insert(id.clone(), self.state.hold_outbox_for_sharing(&id));
                self.claims.insert(
                    id.clone(),
                    LaunchClaim {
                        state: self.state.clone(),
                        id,
                        watches_launch: false,
                    },
                );
            }
        }
        if let Some(entry) = live.sharing_locks.get_mut(&self.account) {
            entry.restarting.extend(claimed);
        }
        Ok(())
    }

    /// The sessions the change restarts that are still claimed for it.
    pub fn restarting(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.claims.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// The claim on `id`, one of [`Self::restarting`].
    pub fn claim(&self, id: &str) -> Option<&LaunchClaim> {
        self.claims.get(id)
    }

    /// Ends the hold on `id`'s queue ahead of its relaunch, leaving what is queued for the launch's
    /// own triggers to write once the agent can take it (its first hook, its trust screen
    /// released), as a resume does: the session is still claimed and without a process, which the
    /// hold's end keeps a queue for.
    pub fn release_queue_for_relaunch(&mut self, id: &str) {
        if let Some(hold) = self.holds.remove(id) {
            hold.forward_to(id);
        }
    }

    /// Lets `id` go: it is no longer restarted, and every client is told its own status again.
    pub fn end_restart(&mut self, id: &str) {
        if let Some(entry) = self
            .state
            .live
            .write()
            .expect("live sessions lock poisoned")
            .sharing_locks
            .get_mut(&self.account)
        {
            entry.restarting.remove(id);
        }
        drop(self.claims.remove(id));
        drop(self.holds.remove(id));
        // A message queued after the queue was handed to the launch and before the session was let
        // go has had no drain to write it, its hooks may all have come already, or, when the launch
        // failed, nobody to drop it.
        if self.state.queued_count(id) > 0 {
            if self.state.has_process(id) {
                if let (Ok(Some(session)), Ok(_)) = (
                    self.state.store.get_session(id),
                    tokio::runtime::Handle::try_current(),
                ) {
                    self.state
                        .spawn_flush_outbox(id, self.state.agent_status(&session));
                }
            } else if !self.state.is_launching(id) {
                self.state.drop_held_queue(id, true);
            }
        }
        if let Ok(Some(session)) = self.state.store.get_session(id) {
            self.state.publish_session(&session);
        }
    }
}

impl Drop for AccountSharingLock {
    fn drop(&mut self) {
        for id in self.restarting() {
            self.end_restart(&id);
        }
        self.state
            .live
            .write()
            .expect("live sessions lock poisoned")
            .sharing_locks
            .remove(&self.account);
    }
}

impl AppState {
    /// [`Self::queue_message`] for a session a change of its account's sharing is restarting, and
    /// `None` for any other. The check and the push are one step under the `live` lock, which
    /// [`AccountSharingLock::end_restart`] takes to let the session go before it ends the hold, so
    /// a message is never pushed after the hold has dropped what it held.
    ///
    /// [`AccountSharingLock::end_restart`]: AccountSharingLock::end_restart
    pub(crate) fn queue_if_restarting(&self, id: &str, text: &str) -> Option<Ticket> {
        let ticket = {
            let live = self.live.read().expect("live sessions lock poisoned");
            live.sharing_locks
                .values()
                .any(|entry| entry.restarting.contains(id))
                .then(|| self.outbox.push(id, text))
        }?;
        // After the lock: it reads the live sessions again.
        self.note_activity(id);
        Some(ticket)
    }

    /// Locks `account` for a change of its sharing, in one step with `admit`, which is shown
    /// whether the account is locked already and the sessions running or holding a claim, and
    /// refuses by answering an error: no claim can be taken or let go in between, and from then on
    /// [`Self::begin_launch`] refuses every session of the account until the answer is dropped.
    pub fn lock_account_sharing<T>(
        self: &Arc<Self>,
        account: &crate::protocol::Account,
        admit: impl FnOnce(bool, &[Running]) -> Result<T>,
    ) -> Result<(T, AccountSharingLock)> {
        let mut live = self.live.write().expect("live sessions lock poisoned");
        let running = self.running_of(&live)?;
        let admitted = admit(live.sharing_locks.contains_key(&account.id), &running)?;
        live.sharing_locks.insert(
            account.id.clone(),
            SharingLockEntry {
                account: account.clone(),
                restarting: HashSet::new(),
            },
        );
        Ok((
            admitted,
            AccountSharingLock {
                state: self.clone(),
                account: account.id.clone(),
                claims: HashMap::new(),
                holds: HashMap::new(),
            },
        ))
    }

    /// The account locked for a change of its sharing that `session` runs under, if any, unless
    /// the session is one that change restarts.
    pub fn sharing_lock_over(&self, session: &Session) -> Option<crate::protocol::Account> {
        let live = self.live.read().expect("live sessions lock poisoned");
        locked_over(&live, session)
            .filter(|entry| !entry.restarting.contains(&session.id))
            .map(|entry| entry.account.clone())
    }

    /// The account `id` while it is locked for a change of its sharing.
    pub fn sharing_locked(&self, id: &str) -> Option<crate::protocol::Account> {
        let live = self.live.read().expect("live sessions lock poisoned");
        live.sharing_locks
            .get(id)
            .map(|entry| entry.account.clone())
    }

    /// Marks `account` as being removed with its directory moved to the Trash, until the answer is
    /// dropped: [`crate::accounts::login::AccountLogins`] refuses to start a sign-in for it
    /// meanwhile, which would write into the directory once its sign-in has ended.
    pub fn begin_account_removal(
        self: &Arc<Self>,
        account: &crate::protocol::Account,
    ) -> AccountRemoval {
        self.accounts_being_removed
            .lock()
            .expect("accounts being removed lock poisoned")
            .entry(account.id.clone())
            .or_insert_with(|| (account.clone(), 0))
            .1 += 1;
        AccountRemoval {
            state: self.clone(),
            account: account.id.clone(),
        }
    }

    /// The account `id` while its removal with its directory moved to the Trash is running.
    pub fn account_being_removed(&self, id: &str) -> Option<crate::protocol::Account> {
        self.accounts_being_removed
            .lock()
            .expect("accounts being removed lock poisoned")
            .get(id)
            .map(|(account, _)| account.clone())
    }
}

#[cfg(test)]
mod tests {
    /// An account is marked as being removed until every removal of it has ended.
    #[test]
    fn an_account_is_being_removed_until_every_removal_has_ended() {
        let (state, _dir) = crate::test_support::console_with_project_and_accounts("state-removal");
        let account = state.store.get_account("claude-a").unwrap().unwrap();
        assert!(state.account_being_removed(&account.id).is_none());
        let first = state.begin_account_removal(&account);
        let second = state.begin_account_removal(&account);
        drop(first);
        assert!(state.account_being_removed(&account.id).is_some());
        drop(second);
        assert!(state.account_being_removed(&account.id).is_none());
    }
}
