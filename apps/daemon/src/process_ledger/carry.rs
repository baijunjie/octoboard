//! What a launch's processes carry over to another launch: the marks that say where they go when
//! the launch ends, the hold that keeps them for a session's next launch, and the carrying itself.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::ProcessLedger;

/// An open hold for the next launch of a session (see [`ProcessLedger::hold_for_launch`]).
/// Dropped, as the last one for the session, it gives up what is still held for the session, to
/// be stopped, and what was marked to be carried over to it by the session's own launch and has
/// not ended yet is stopped when it does. A mark on another session's launch is left: a handoff
/// may have made it meanwhile. Giving up what is kept writes the ledger file, so a caller on the
/// runtime drops it on a blocking task.
pub struct HeldForLaunch {
    ledger: Arc<ProcessLedger>,
    session_id: String,
    /// The project what is still held is kept under when the hold is given up, instead of being
    /// stopped (see [`Self::keep_what_is_held`]).
    keep_in: Option<String>,
}

impl HeldForLaunch {
    /// Gives the hold up, making what is still held for the session the user's instead of
    /// stopping it: kept, as a tree the user keeps is, and listed among the processes of the
    /// project `project`. A restart (an account switch, a model change, a change of an
    /// account's sharing) that leaves its session interrupted does this. A daemon shutting down
    /// ([`ProcessLedger::close`]) keeps nothing: it is stopped all the same.
    pub fn keep_what_is_held(mut self, project: String) {
        self.keep_in = Some(project);
    }
}

impl Drop for HeldForLaunch {
    fn drop(&mut self) {
        // A poisoned book is left as it is: a panic in a drop while unwinding would abort.
        let Ok(mut book) = self.ledger.book.lock() else {
            return;
        };
        let session = self.session_id.as_str();
        let open = match book.awaited.get_mut(session) {
            Some(count) if *count > 1 => {
                *count -= 1;
                true
            }
            _ => {
                book.awaited.remove(session);
                false
            }
        };
        if open {
            return;
        }
        let held: Vec<String> = book
            .launches
            .iter()
            .filter(|(_, launch)| launch.held && launch.session_id.as_deref() == Some(session))
            .map(|(id, _)| id.clone())
            .collect();
        for launch in book.launches.values_mut() {
            if launch.heir.as_deref() == Some(session)
                && launch.session_id.as_deref() == Some(session)
                && !launch.ending
            {
                launch.heir = None;
            }
        }
        let keep_in = self
            .keep_in
            .as_ref()
            .filter(|_| !self.ledger.closing.load(Ordering::Acquire));
        for launch in &held {
            match keep_in {
                Some(project) => book.keep_launch(launch, project),
                None => book.hand_to_ended(launch),
            }
        }
        drop(book);
        if !held.is_empty() {
            if keep_in.is_some() {
                tracing::info!(
                    session = %self.session_id,
                    "the processes held for a session's next launch are kept for the user, as it did not come up"
                );
                self.ledger.persist(true);
            } else {
                tracing::info!(
                    session = %self.session_id,
                    "the processes held for a session's next launch are stopped, as it did not start"
                );
            }
            self.ledger.forget_lists();
        }
    }
}

impl ProcessLedger {
    /// Marks the running launch `launch_id` to have its processes carried over to the launch of
    /// the session `heir` when its agent process ends, instead of being stopped (see
    /// [`Self::end_launch`]): the session that takes its place in a handoff, or its own session
    /// for the relaunch of a restart (an account switch, a model change, a change of an account's
    /// sharing). Answers whether the launch was marked: one that is not followed, has not started
    /// or is ending already is not.
    pub fn hand_over(&self, launch_id: &str, heir: &str) -> bool {
        let mut book = self.book();
        match book.launches.get_mut(launch_id) {
            Some(launch) if launch.started && !launch.ending && !launch.held => {
                launch.heir = Some(heir.to_string());
                true
            }
            _ => false,
        }
    }

    /// Takes back [`Self::hand_over`] for the launch `launch_id` while it has not ended yet: its
    /// processes are stopped when it ends, as any launch's.
    pub fn withdraw_hand_over(&self, launch_id: &str) {
        let mut book = self.book();
        if let Some(launch) = book.launches.get_mut(launch_id) {
            if !launch.ending {
                launch.heir = None;
            }
        }
    }

    /// Holds what a launch carries over to the session `session_id` (see [`Self::hand_over`])
    /// while the session has no launch running, for its next launch to take in when it starts
    /// ([`Self::launch_started`]), for as long as the answer is kept: a restart keeps it across
    /// the gap between the end of the process it stops and the start of its relaunch.
    pub fn hold_for_launch(self: &Arc<Self>, session_id: &str) -> HeldForLaunch {
        *self
            .book()
            .awaited
            .entry(session_id.to_string())
            .or_default() += 1;
        HeldForLaunch {
            ledger: self.clone(),
            session_id: session_id.to_string(),
            keep_in: None,
        }
    }

    /// Carries nothing over from here on, for a daemon that is shutting down: its sessions are
    /// all ended, and none is relaunched. What is held already is stopped by
    /// [`Self::end_held_launches`], and a hold given up from here on stops what it holds rather
    /// than keeping it.
    pub fn close(&self) {
        self.closing.store(true, Ordering::Release);
    }

    /// Stops what is held for sessions' next launches, as [`Self::end_launch`] stops a launch's
    /// processes, for a daemon shutting down after [`Self::close`]. **Blocks.**
    pub fn end_held_launches(&self) {
        let held: Vec<String> = {
            let mut book = self.book();
            let held: Vec<String> = book
                .launches
                .iter()
                .filter(|(_, launch)| launch.held && !launch.ending)
                .map(|(id, _)| id.clone())
                .collect();
            for id in &held {
                if let Some(launch) = book.launches.get_mut(id) {
                    launch.held = false;
                    launch.ending = true;
                }
            }
            held
        };
        for launch in held {
            self.stop_launch(&launch);
        }
    }

    /// Carries the processes of `launch_id`, which is ending and was marked to be carried over,
    /// to the launch of the session it was marked for (see [`Self::end_launch`]). Answers whether
    /// it did; when it did not, the launch is still ending, to be stopped.
    pub(super) fn carry_over(&self, launch_id: &str) -> bool {
        let mut book = self.book();
        let Some(heir) = book.heir_of(launch_id) else {
            return false;
        };
        if self.closing.load(Ordering::Acquire) {
            return false;
        }
        match book.running_launch_of(&heir, launch_id) {
            Some(receiving) => {
                book.transfer(launch_id, &receiving, None);
                tracing::info!(
                    launch = launch_id,
                    session = %heir,
                    "carrying what an ended launch left running over to the session that takes its place"
                );
            }
            None if book.awaited.contains_key(&heir) => {
                let Some(launch) = book.launches.get_mut(launch_id) else {
                    return false;
                };
                // Its agent is gone: what it started is followed by its chain no more, and what
                // is in its session was swept with it.
                launch.agent = None;
                launch.ending = false;
                launch.held = true;
                launch.heir = None;
                launch.session_id = Some(heir.clone());
                book.dirty = true;
                tracing::info!(
                    launch = launch_id,
                    session = %heir,
                    "holding what an ended launch left running for the session's next launch"
                );
            }
            None => return false,
        }
        drop(book);
        self.forget_lists();
        self.launch_left.notify_all();
        self.persist(true);
        true
    }
}
