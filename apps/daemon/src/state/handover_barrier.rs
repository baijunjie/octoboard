//! The barrier a planned stop raises before it takes the list of terminals to hand over: once
//! raised, nothing more is admitted that would change or end a terminal, or remove what one works
//! in, and the operations admitted before it are waited for, for a bounded time, so that none of
//! them acts on a terminal between its handover mark and the parting.
//!
//! An operation that can do so takes an admission ([`AppState::admit_change`]) at its shared entry
//! point, before it changes any record or touches the file system, and keeps it until it is done:
//! archiving or deleting sessions, removing a project or a console, a restart for a change of
//! account, model or sharing, a handoff, an explicit stop of processes, creating or closing a shell
//! terminal, an interrupt, a write from the outbox. Once the barrier is raised a new admission is
//! refused with `daemon_shutting_down`, including one an operation already admitted asks for at a
//! later step of its own, which stops it there; a step of an operation whose own claim already kept
//! it from starting after the barrier (the start of a process under a launch or switch claim, which
//! [`AppState::refuse_while_shutting_down`] refuses once the barrier is raised) is only counted
//! ([`AppState::track_change`]), so that it is waited for but never cut off half-way; the start of
//! a process is refused, though, once the wait has run out
//! ([`AppState::track_change_unless_passed`]), and the end of one an operation began is counted
//! until the process has ended. What an admitted operation does after its own step failed, such as
//! bringing back a session its failed switch left down, claims the session with proof of the
//! admission ([`AppState::begin_account_switch_admitted`]), which the raised barrier does not
//! refuse. Removing the branches of archived or deleted sessions, which runs as a task of its own,
//! is counted from while the operation was still admitted, and does not start once the wait has
//! run out.
//!
//! An operation still admitted when the wait runs out goes on with the planned stop no longer
//! waiting for it; before an act that cannot be undone, such as moving a folder to the Trash, it
//! asks whether the barrier still covers it ([`ChangeAdmission::still_covered`]). An ordinary stop
//! raises no barrier.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::watch;

use super::AppState;
use crate::protocol::{error_code, CodedError};

/// Where the barrier stands.
#[derive(Debug, Default)]
pub(super) struct Barrier {
    /// Raised by a planned stop: no admission is granted from then on.
    raised: bool,
    /// The planned stop no longer waits for the admissions still held.
    passed: bool,
    /// The admissions held now.
    admitted: usize,
}

/// The barrier as [`AppState`] holds it, shared with every admission so that dropping one needs
/// no reference to the state.
pub(super) type HandoverBarrier = Arc<watch::Sender<Barrier>>;

pub(super) fn new_barrier() -> HandoverBarrier {
    Arc::new(watch::channel(Barrier::default()).0)
}

/// An operation's admission past the handover barrier, held until it is done and released on
/// `Drop`, so that an operation that fails or panics part-way never keeps a planned stop waiting.
#[derive(Debug)]
#[must_use = "the admission covers the operation only while it is held"]
pub struct ChangeAdmission {
    barrier: HandoverBarrier,
}

impl ChangeAdmission {
    /// Refuses, as a new admission is refused, once a planned stop has stopped waiting for the
    /// operation holding this admission: from then on the terminals may be handed over, and an act
    /// that cannot be undone would happen behind the handover's back. Asked right before such an
    /// act.
    pub fn still_covered(&self) -> Result<()> {
        if self.barrier.borrow().passed {
            return Err(refusal());
        }
        Ok(())
    }
}

impl Drop for ChangeAdmission {
    fn drop(&mut self) {
        self.barrier
            .send_modify(|barrier| barrier.admitted = barrier.admitted.saturating_sub(1));
    }
}

/// The refusal of an admission, the code an ordinary stop refuses with too.
fn refusal() -> anyhow::Error {
    CodedError::raised(
        error_code::DAEMON_SHUTTING_DOWN,
        "the daemon is stopping to hand its terminals over to the next daemon, so nothing that \
         would change or end one is done now",
        &[],
    )
}

impl AppState {
    /// Admits an operation that may change or end a terminal, or remove what one works in, for as
    /// long as the admission is held; refused with `daemon_shutting_down` once a planned stop has
    /// raised the barrier. Taken before the operation changes anything.
    pub fn admit_change(&self) -> Result<ChangeAdmission> {
        let admitted = self.handover_barrier.send_if_modified(|barrier| {
            if barrier.raised {
                return false;
            }
            barrier.admitted += 1;
            true
        });
        if !admitted {
            return Err(refusal());
        }
        Ok(ChangeAdmission {
            barrier: self.handover_barrier.clone(),
        })
    }

    /// Counts a step that changes a terminal and that the barrier must wait for but never refuses:
    /// one the operation's own claim, refused once the barrier is raised, has already admitted.
    pub fn track_change(&self) -> ChangeAdmission {
        self.handover_barrier
            .send_modify(|barrier| barrier.admitted += 1);
        ChangeAdmission {
            barrier: self.handover_barrier.clone(),
        }
    }

    /// [`Self::track_change`] for a step that must not begin once a planned stop has stopped
    /// waiting, such as the start of a process: a launch claimed before the barrier went up and
    /// still starting when the wait ran out would otherwise start a terminal the stop never lists.
    /// Refused with `daemon_shutting_down` then; counted, never refused, while the stop waits.
    pub fn track_change_unless_passed(&self) -> Result<ChangeAdmission> {
        let counted = self.handover_barrier.send_if_modified(|barrier| {
            if barrier.passed {
                return false;
            }
            barrier.admitted += 1;
            true
        });
        if !counted {
            return Err(refusal());
        }
        Ok(ChangeAdmission {
            barrier: self.handover_barrier.clone(),
        })
    }

    /// Whether a planned stop has raised the barrier, after which nothing more is admitted.
    pub fn handover_barrier_raised(&self) -> bool {
        self.handover_barrier.borrow().raised
    }

    /// Raises the barrier and waits, for at most `wait`, until no admission is held; whether none
    /// is. Whatever is still admitted then goes on without the planned stop waiting for it.
    pub(super) async fn raise_handover_barrier(&self, wait: Duration) -> bool {
        self.handover_barrier
            .send_modify(|barrier| barrier.raised = true);
        let mut changes = self.handover_barrier.subscribe();
        // The wait's borrow is gone with the statement, before the barrier is written again.
        let drained = matches!(
            tokio::time::timeout(wait, changes.wait_for(|barrier| barrier.admitted == 0)).await,
            Ok(Ok(_))
        );
        self.handover_barrier
            .send_modify(|barrier| barrier.passed = true);
        drained
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::protocol::error_code;
    use crate::test_support::{app_state, code_of, PATIENCE};

    /// Once raised, the barrier admits nothing more, an operation already admitted asking again
    /// included, while a step it only counts still goes.
    #[tokio::test]
    async fn nothing_is_admitted_once_the_barrier_is_raised() {
        let (state, _dir) = app_state("barrier-refuses");
        assert!(state.admit_change().is_ok());
        assert!(!state.handover_barrier_raised());
        assert!(state.raise_handover_barrier(PATIENCE).await);
        assert!(state.handover_barrier_raised());
        let err = state.admit_change().expect_err("refused");
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
        let tracked = state.track_change();
        drop(tracked);
    }

    /// The planned stop waits for what was admitted before the barrier went up, and goes on as
    /// soon as the last admission is released.
    #[tokio::test]
    async fn raising_the_barrier_waits_for_what_was_admitted() {
        let (state, _dir) = app_state("barrier-waits");
        let admitted = state.admit_change().unwrap();
        let tracked = state.track_change();
        let releasing = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(tracked);
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(admitted);
        });
        let started = std::time::Instant::now();
        assert!(state.raise_handover_barrier(PATIENCE).await);
        assert!(started.elapsed() >= Duration::from_millis(200));
        releasing.await.unwrap();
    }

    /// An operation the wait gave up on is told so before an act that cannot be undone; one the
    /// barrier still waits for is not.
    #[tokio::test]
    async fn an_admission_outlasting_the_wait_is_no_longer_covered() {
        let (state, _dir) = app_state("barrier-gives-up");
        let admitted = state.admit_change().unwrap();
        assert!(admitted.still_covered().is_ok());
        assert!(
            !state
                .raise_handover_barrier(Duration::from_millis(50))
                .await
        );
        let err = admitted.still_covered().expect_err("no longer covered");
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
    }

    /// A step that must not begin once the stop has stopped waiting, such as starting a process
    /// under a claim taken before the barrier went up, is counted while the stop waits, and
    /// refused once it no longer does.
    #[tokio::test]
    async fn a_step_that_begins_after_the_wait_ran_out_is_refused() {
        let (state, _dir) = app_state("barrier-step-after-wait");
        let admitted = state.admit_change().unwrap();
        let waiting = {
            let state = state.clone();
            tokio::spawn(async move {
                state
                    .raise_handover_barrier(Duration::from_millis(300))
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(state.handover_barrier_raised());
        let counted = state
            .track_change_unless_passed()
            .expect("counted while the stop waits");
        assert!(
            !waiting.await.unwrap(),
            "the stop waited for the counted step"
        );
        drop(counted);
        let err = state
            .track_change_unless_passed()
            .expect_err("refused once the stop no longer waits");
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
        drop(admitted);
    }

    /// An operation admitted before the barrier goes up can claim the session again to finish what
    /// it began, while the same claim is refused to everything else, and to the operation itself
    /// once the planned stop has stopped waiting for it.
    #[tokio::test]
    async fn an_admitted_operation_can_claim_a_session_again_to_finish() {
        let (state, _dir) = app_state("barrier-admitted-claim");
        let admitted = state.admit_change().unwrap();
        let raising = tokio::spawn({
            let state = state.clone();
            async move {
                state
                    .raise_handover_barrier(Duration::from_millis(300))
                    .await
            }
        });
        while !state.handover_barrier_raised() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let err = state
            .begin_account_switch("s", true)
            .err()
            .expect("refused");
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
        let claim = state
            .begin_account_switch_admitted("s", true, &admitted)
            .expect("the admitted operation finishes");
        drop(claim);
        assert!(!raising.await.unwrap());
        let err = state
            .begin_account_switch_admitted("s", true, &admitted)
            .err()
            .expect("no longer covered");
        assert_eq!(code_of(&err), error_code::DAEMON_SHUTTING_DOWN);
    }

    /// An ordinary stop raises no barrier: operations are still admitted while it ends the
    /// sessions.
    #[tokio::test]
    async fn an_ordinary_stop_raises_no_barrier() {
        let (state, _dir) = app_state("barrier-ordinary-stop");
        state.stop_all_sessions().await;
        assert!(!state.handover_barrier_raised());
        assert!(state.admit_change().is_ok());
    }
}
