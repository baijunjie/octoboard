//! The retries of a Codex turn that ended because the selected model was at capacity: each one a
//! prompt written into the same running process after 30 s, 60 s, then 120 s, never a relaunch or
//! a change of model, effort or account. See `docs/product/turns-hooks-do-not-report.md`, "A Codex
//! turn that ends in an error", for what the user and the owner see.
//!
//! [`CapacityRetries`] holds, per session, how many capacity failures in a row there have been and
//! the wait for the next retry. The count is what ends the sequence, so it is kept apart from the
//! retry prompt: it restarts only when a turn ends without a capacity failure, or when the
//! session's process or record is acted on ([`Cause::Report`]).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::messaging::delivery::{self, Delivery, WhenBlocked};
use crate::messaging::texts::capacity_retry_prompt;
use crate::messaging::{owner_notices, reporting};
use crate::protocol::SessionStatus;
use crate::state::session_as_left::SessionAsLeft;
use crate::state::AppState;

/// How long each retry waits before it is sent, in order; the retries are as many as this has.
const DELAYS: [Duration; 3] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

/// Why a wait was ended from outside.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// A turn started or a message was queued for one: the session is at work again and reports
    /// for itself, so the wait ends and nobody is told.
    Silent,
    /// Something was done to the process, the record or the turn that opens no turn: the owner is
    /// given the failed report that was held back, and the sequence starts again.
    Report,
}

/// What a capacity failure comes to.
pub(crate) enum Judgement {
    Retry(Plan),
    /// The retries are used up.
    Exhausted,
}

/// A retry to arm: which of the retries it is (from 1) and how long to wait before it.
pub(crate) struct Plan {
    attempt: usize,
    delay: Duration,
}

/// The delay before the retry that follows `failures` capacity failures in a row, or `None` when
/// there are no retries left.
fn delay_after(failures: usize) -> Option<Duration> {
    DELAYS.get(failures).copied()
}

/// The waits of all sessions, by session id.
#[derive(Default)]
pub(crate) struct CapacityRetries {
    entries: Mutex<HashMap<String, Entry>>,
    /// Stands in for every delay, so that a test does not wait out the real ones.
    #[cfg(test)]
    delay: Mutex<Option<Duration>>,
}

#[derive(Default)]
struct Entry {
    /// Capacity failures in a row, the one being retried included.
    failures: usize,
    /// The wait for the next retry, while there is one.
    pending: Option<Pending>,
    /// The retry is being written into the session.
    sending: bool,
    /// The wait was ended for [`Cause::Report`] while the retry was being written.
    stopped: bool,
    /// The turn being closed is the capacity failure just judged, which does not end the count.
    closing_at_capacity: bool,
}

/// A wait for a retry, with the session as the failure left it.
#[derive(Clone)]
struct Pending {
    /// Tells this wait from another of the same session.
    token: Arc<()>,
    signal: Arc<Signal>,
    attempt: usize,
    delay: Duration,
    /// The session's owner is owed a report of the failure, and was told of the retry in its place.
    owed: bool,
    /// The error text of the failed turn.
    message: Option<String>,
    left: SessionAsLeft,
    /// The delivery of the notice to the owner, which the failed report must not overtake.
    notice: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Pending {
    fn take_notice(&self) -> Option<JoinHandle<()>> {
        self.notice.lock().expect("notice lock poisoned").take()
    }
}

/// Wakes a wait that is ended from outside, and says why.
#[derive(Default)]
struct Signal {
    woken: Notify,
    report: AtomicBool,
}

impl Signal {
    fn wake(&self, cause: Cause) {
        self.report.store(cause == Cause::Report, Ordering::Release);
        self.woken.notify_one();
    }

    fn is_report(&self) -> bool {
        self.report.load(Ordering::Acquire)
    }
}

impl CapacityRetries {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().expect("capacity retries lock poisoned")
    }

    /// Counts the capacity failure of the session `id`'s turn that has just ended, and says whether
    /// to retry it. Replaces any wait still pending. The turn is to be closed next, which does not
    /// end the count ([`Self::turn_ended`]).
    pub(crate) fn judge(&self, id: &str) -> Judgement {
        let mut entries = self.lock();
        let entry = entries.entry(id.to_string()).or_default();
        if let Some(pending) = entry.pending.take() {
            pending.signal.wake(Cause::Silent);
        }
        let Some(delay) = delay_after(entry.failures) else {
            entries.remove(id);
            return Judgement::Exhausted;
        };
        entry.failures += 1;
        entry.closing_at_capacity = true;
        Judgement::Retry(Plan {
            attempt: entry.failures,
            delay: self.scaled(delay),
        })
    }

    #[cfg(not(test))]
    fn scaled(&self, delay: Duration) -> Duration {
        delay
    }

    #[cfg(test)]
    fn scaled(&self, delay: Duration) -> Duration {
        self.delay.lock().unwrap().unwrap_or(delay)
    }

    /// Has every delay be `delay` instead, under test.
    #[cfg(test)]
    pub(crate) fn set_delay_for_test(&self, delay: Duration) {
        *self.delay.lock().unwrap() = Some(delay);
    }

    /// Whether the session `id` has a retry pending, under test.
    #[cfg(test)]
    pub(crate) fn is_waiting_for_test(&self, id: &str) -> bool {
        self.lock()
            .get(id)
            .is_some_and(|entry| entry.pending.is_some())
    }

    /// The capacity failures in a row held for the session `id`, under test.
    #[cfg(test)]
    pub(crate) fn failures_for_test(&self, id: &str) -> usize {
        self.lock().get(id).map_or(0, |entry| entry.failures)
    }

    /// Drops the session's count and wait, for a failure that could not be retried after all.
    pub(crate) fn forget(&self, id: &str) {
        self.lock().remove(id);
    }

    /// Holds `pending` as the session's wait. `false` when the entry `judge` made was ended
    /// meanwhile, by a cancel that came before there was a wait to wake.
    fn set_pending(&self, id: &str, pending: Pending) -> bool {
        match self.lock().get_mut(id) {
            Some(entry) => {
                entry.pending = Some(pending);
                true
            }
            None => false,
        }
    }

    /// Ends the session `id`'s wait for `cause`, and wakes it to say so.
    pub(crate) fn cancel(&self, id: &str, cause: Cause) {
        let mut entries = self.lock();
        let Some(entry) = entries.get_mut(id) else {
            return;
        };
        match cause {
            // The count stays: whether the session got over the capacity failure is for the end of
            // the turn it is now at to say.
            Cause::Silent => {
                if let Some(pending) = entry.pending.take() {
                    pending.signal.wake(cause);
                }
            }
            // A retry being written cannot be taken back, so it is ended once it is written.
            Cause::Report if entry.sending => entry.stopped = true,
            Cause::Report => {
                if let Some(Entry {
                    pending: Some(pending),
                    ..
                }) = entries.remove(id)
                {
                    pending.signal.wake(cause);
                }
            }
        }
    }

    /// A turn of the session `id` was closed. Unless it is the capacity failure just judged, the
    /// session got over the capacity failures, and the count starts again. A wait pending is not
    /// for this to touch.
    pub(crate) fn turn_ended(&self, id: &str) {
        let mut entries = self.lock();
        let Some(entry) = entries.get_mut(id).filter(|entry| entry.pending.is_none()) else {
            return;
        };
        if std::mem::take(&mut entry.closing_at_capacity) {
            return;
        }
        entry.failures = 0;
        if !entry.sending {
            entries.remove(id);
        }
    }

    /// Takes the wait `token` out to be acted on, and marks its retry as being written. `None`
    /// when it was ended meanwhile.
    fn claim(&self, id: &str, token: &Arc<()>) -> Option<Pending> {
        let mut entries = self.lock();
        let entry = entries.get_mut(id)?;
        if !entry
            .pending
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(&pending.token, token))
        {
            return None;
        }
        entry.sending = true;
        entry.stopped = false;
        entry.pending.take()
    }

    /// The retry was written or not. Whether it stands: a retry not written, or one whose wait was
    /// ended for [`Cause::Report`] while it was, leaves no sequence.
    fn finish_send(&self, id: &str, written: bool) -> bool {
        let mut entries = self.lock();
        let stopped = entries.get_mut(id).is_none_or(|entry| {
            entry.sending = false;
            std::mem::take(&mut entry.stopped)
        });
        if written && !stopped {
            return true;
        }
        entries.remove(id);
        false
    }

    /// Drops the session's entry.
    fn drop_entry(&self, id: &str) {
        self.lock().remove(id);
    }
}

/// Starts the wait for retry `plan` of the session `id`, whose failed turn has just been closed and
/// which is back at its prompt, and, when its owner is owed a report of the failure (`owed`), tells
/// the owner in its place that a retry is coming. `message` is the failed turn's error text. Runs
/// on the runtime.
pub(crate) fn arm(
    state: &Arc<AppState>,
    id: &str,
    plan: Plan,
    message: Option<String>,
    owed: bool,
) {
    let left = match SessionAsLeft::capture(state, id) {
        Ok(Some(left)) => left,
        _ => {
            state.capacity_retries.forget(id);
            give_up(state, id, message, owed, Outcome::Stopped, None);
            return;
        }
    };
    let pending = Pending {
        token: Arc::new(()),
        signal: Arc::new(Signal::default()),
        attempt: plan.attempt,
        delay: plan.delay,
        owed,
        message,
        left,
        notice: Arc::default(),
    };
    if !state.capacity_retries.set_pending(id, pending.clone()) {
        give_up(state, id, pending.message, owed, Outcome::Stopped, None);
        return;
    }
    if owed {
        let (state, session) = (state.clone(), (*pending.left).clone());
        let (attempt, seconds) = (pending.attempt, pending.delay.as_secs());
        // Delivery writes into the owner's PTY, which blocks.
        let notice = tokio::task::spawn_blocking(move || {
            owner_notices::tell_owner_it_will_be_retried(
                &state,
                &session,
                attempt,
                DELAYS.len(),
                seconds,
            )
        });
        *pending.notice.lock().expect("notice lock poisoned") = Some(notice);
    }
    tokio::spawn(wait_then_retry(state.clone(), id.to_string(), pending));
}

/// Waits out `pending`, then sends the retry if the session is still as the failure left it, and
/// gives the owner the failed report if it is not, or if the wait was ended with [`Cause::Report`].
async fn wait_then_retry(state: Arc<AppState>, id: String, pending: Pending) {
    tokio::select! {
        _ = tokio::time::sleep(pending.delay) => {}
        _ = pending.signal.woken.notified() => {}
    }
    let retries = &state.capacity_retries;
    let Some(claimed) = retries.claim(&id, &pending.token) else {
        // Ended from outside; a silent end has nothing more to say.
        if pending.signal.is_report() {
            let notice = pending.take_notice();
            give_up(
                &state,
                &id,
                pending.message,
                pending.owed,
                Outcome::Stopped,
                notice,
            );
        }
        return;
    };
    if !still_waiting(&state, &id, &claimed) {
        retries.drop_entry(&id);
        let notice = claimed.take_notice();
        give_up(
            &state,
            &id,
            claimed.message,
            claimed.owed,
            Outcome::Stopped,
            notice,
        );
        return;
    }
    let written = {
        let (state, id) = (state.clone(), id.clone());
        let text = capacity_retry_prompt(claimed.attempt, DELAYS.len());
        tokio::task::spawn_blocking(move || {
            // Refused rather than queued: a session that cannot take it now is not at the prompt
            // the failure left it at.
            matches!(
                delivery::write_message(&state, &id, &text, WhenBlocked::Refuse),
                Ok(Delivery::Written)
            )
        })
        .await
        .unwrap_or(false)
    };
    if !retries.finish_send(&id, written) {
        let notice = claimed.take_notice();
        give_up(
            &state,
            &id,
            claimed.message,
            claimed.owed,
            Outcome::Stopped,
            notice,
        );
    }
}

/// Whether the session `id` is as `pending` found it, and idle at its prompt: no turn open, not
/// claimed by a launch. A session that waits for the user, shows as dormant, was archived or
/// deleted is not.
fn still_waiting(state: &Arc<AppState>, id: &str, pending: &Pending) -> bool {
    !state.is_shutting_down()
        && !state.turn_is_open(id)
        && !state.is_launching(id)
        && pending
            .left
            .unchanged(state)
            .is_some_and(|session| state.shown_status(&session) == SessionStatus::Idle)
}

/// Why the owner is given the failed report after all.
#[derive(Clone, Copy)]
pub(crate) enum Outcome {
    /// Every retry was sent and the model was still at capacity.
    Exhausted,
    /// The retry did not go ahead.
    Stopped,
}

/// The text of the failed report for a turn whose error text was `message`, saying what became of
/// the retries.
pub(crate) fn failure_text(message: Option<String>, outcome: Outcome) -> String {
    let error = message.unwrap_or_else(|| "The selected model was at capacity.".to_string());
    let what_became = match outcome {
        Outcome::Exhausted => format!(
            "{} retried automatically {} times and the model was still at capacity.",
            crate::APP_NAME,
            DELAYS.len()
        ),
        Outcome::Stopped => format!("{}'s automatic retry did not go ahead.", crate::APP_NAME),
    };
    format!(
        "{error} {what_became} The user can retry later or switch the session's model manually."
    )
}

/// Gives the owner the failed report that was held back for the retry, when one was owed and the
/// session is still there, after the `notice` of the retry has been delivered, which it must not
/// overtake. Runs on the runtime; the delivery goes off it.
fn give_up(
    state: &Arc<AppState>,
    id: &str,
    message: Option<String>,
    owed: bool,
    outcome: Outcome,
    notice: Option<JoinHandle<()>>,
) {
    if !owed {
        return;
    }
    let (state, id) = (state.clone(), id.to_string());
    tokio::spawn(async move {
        if let Some(notice) = notice {
            let _ = notice.await;
        }
        let _ = tokio::task::spawn_blocking(move || {
            let present = matches!(
                state.store.get_session(&id),
                Ok(Some(session)) if session.status != SessionStatus::Archived
            );
            if present {
                reporting::report_failed_turn(&state, &id, Some(failure_text(message, outcome)));
            }
        })
        .await;
    });
}

#[cfg(test)]
mod tests;
