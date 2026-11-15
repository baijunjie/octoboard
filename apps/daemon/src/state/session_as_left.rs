//! A session as a failed turn left it, to tell later whether it is still as it was: the process,
//! how much was written into it, how many turns ended, and the record's account, model and
//! conversation. What a wait for a recovery of the session is allowed to act on.

use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;

use super::AppState;
use crate::protocol::Session;
use crate::session::LiveSession;

#[derive(Clone)]
pub(crate) struct SessionAsLeft {
    record: Session,
    live: Arc<LiveSession>,
    input_writes: u64,
    turn_ends: u64,
}

impl Deref for SessionAsLeft {
    type Target = Session;

    fn deref(&self) -> &Session {
        &self.record
    }
}

impl SessionAsLeft {
    /// The session `id` as it stands, or `None` when it has no process.
    pub(crate) fn capture(state: &AppState, id: &str) -> Result<Option<Self>> {
        let Some(live) = state.live_session(id) else {
            return Ok(None);
        };
        let record = state.session_record(id)?;
        Ok(Some(Self {
            input_writes: live.input_writes(),
            turn_ends: state.turn_ends(id),
            record,
            live,
        }))
    }

    /// The record as it stands now, when the session is still the same process with nothing
    /// written into it and no turn ended since, on the same account, model, effort and
    /// conversation. Its status is for the caller to judge.
    pub(crate) fn unchanged(&self, state: &AppState) -> Option<Session> {
        let id = &self.record.id;
        let same_process = state
            .live_session(id)
            .is_some_and(|live| Arc::ptr_eq(&live, &self.live))
            // Not `poll_exit`: callers ask on the async runtime, and the poll that reaps the
            // process runs its exit cleanup, which is the exit watcher's to run off it.
            && !self.live.has_exited()
            && self.live.input_writes() == self.input_writes
            && state.turn_ends(id) == self.turn_ends;
        if !same_process {
            return None;
        }
        let now = state.store.get_session(id).ok().flatten()?;
        (now.account_id == self.record.account_id
            && now.config_dir == self.record.config_dir
            && now.agent_session_id == self.record.agent_session_id
            && now.model == self.record.model
            && now.effort == self.record.effort)
            .then_some(now)
    }
}

#[cfg(test)]
mod tests {
    use super::SessionAsLeft;
    use crate::protocol::{AgentId, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session_running, programs, HeldExitHook};
    use std::time::{Duration, Instant};

    /// A process that has exited is no longer the session as left, and telling so runs none of
    /// its exit cleanup: callers ask on the async runtime.
    #[tokio::test]
    async fn an_exited_process_is_not_as_left_and_its_exit_cleanup_waits_for_the_watcher() {
        let (state, dir) = app_state("session-as-left-exited");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session_running("s", AgentId::CLAUDE, 80, 24, &programs::sleep(0));
        let hook = HeldExitHook::install(&live, &dir);
        state.register_live(live.clone());
        let left = SessionAsLeft::capture(&state, "s")
            .unwrap()
            .expect("the session has a process");

        let deadline = Instant::now() + crate::test_support::PATIENCE;
        while !live.has_exited() {
            assert!(Instant::now() < deadline, "the stand-in never exited");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(left.unchanged(&state).is_none());
        assert!(!hook.has_run(), "the exit cleanup ran on the runtime");
    }

    /// A state holding session `s`, with a process that stays up, captured as it stands.
    fn session_left(
        label: &str,
    ) -> (
        std::sync::Arc<crate::state::AppState>,
        crate::test_support::StandIn,
        crate::test_support::ScratchDir,
        SessionAsLeft,
    ) {
        let (state, dir) = app_state(label);
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session_running("s", AgentId::CLAUDE, 80, 24, &programs::sleep(600));
        state.register_live(live.clone());
        let left = SessionAsLeft::capture(&state, "s")
            .unwrap()
            .expect("the session has a process");
        (state, live, dir, left)
    }

    #[test]
    fn a_session_with_no_process_is_not_captured() {
        let (state, dir) = app_state("session-as-left-no-process");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        assert!(SessionAsLeft::capture(&state, "s").unwrap().is_none());
    }

    #[test]
    fn a_running_session_nothing_has_touched_is_unchanged_and_reads_as_its_record() {
        let (state, _live, _dir, left) = session_left("session-as-left-untouched");
        assert_eq!(left.id, "s", "it reads as the record it captured");
        let now = left.unchanged(&state).expect("nothing has changed");
        assert_eq!(now.id, "s");
        assert_eq!(
            now.status,
            SessionStatus::Idle,
            "the status is the caller's"
        );
    }

    #[test]
    fn a_session_written_into_is_no_longer_as_left() {
        let (state, live, _dir, left) = session_left("session-as-left-written");
        live.write_input(b"x").expect("the write");
        assert!(left.unchanged(&state).is_none());
    }

    #[test]
    fn a_session_whose_turn_ended_is_no_longer_as_left() {
        let (state, _live, _dir, left) = session_left("session-as-left-turn-ended");
        state.note_turn_end("s");
        assert!(left.unchanged(&state).is_none());
    }

    #[test]
    fn a_session_that_changed_its_account_or_model_is_no_longer_as_left() {
        let (state, _live, _dir, left) = session_left("session-as-left-reconfigured");
        state
            .store
            .set_session_account("s", Some("other"), Some("/other"))
            .unwrap();
        assert!(left.unchanged(&state).is_none());

        let (state, _live, _dir, left) = session_left("session-as-left-model");
        state
            .store
            .set_session_launch_overrides("s", &Some(Some("other-model".to_owned())), &None)
            .unwrap();
        assert!(left.unchanged(&state).is_none());
    }

    #[test]
    fn a_session_whose_process_was_replaced_or_dropped_is_no_longer_as_left() {
        let (state, live, _dir, left) = session_left("session-as-left-replaced");
        assert!(state.remove_live(&live));
        assert!(left.unchanged(&state).is_none());
        let again = fake_live_session_running("s", AgentId::CLAUDE, 80, 24, &programs::sleep(600));
        state.register_live(again.clone());
        assert!(
            left.unchanged(&state).is_none(),
            "another process is not the one left"
        );
    }
}
