//! The hand a waiting session request raises on its session, and the status the session's agent
//! is at meanwhile.

use std::sync::Arc;

use anyhow::Result;

use super::AppState;
use crate::protocol::SessionStatus;

impl AppState {
    /// Raises the hand of a session whose request for a session has started waiting, remembering
    /// the status its agent is at. A session with a request already held keeps what it remembered:
    /// the request replacing another one is the same wait. A record without a process is left
    /// alone, as in [`Self::apply_hook_status`].
    pub fn raise_request_hand(&self, id: &str) -> Result<()> {
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        let session = self.session_record(id)?;
        if session.status.has_no_process() {
            return Ok(());
        }
        hands.entry(id.to_string()).or_insert(session.status);
        // The agent's own status has not moved, so how its last turn ended stays as it was.
        if let Some(session) = self.store.hold_waiting_user(id)? {
            self.publish_session(&session);
        }
        Ok(())
    }

    /// Lowers the hand [`Self::raise_request_hand`] raised, to the status the agent's events have
    /// left it at since, unless another request of the session is still waiting. Conditional on
    /// the hand still being up, so a session that has since been interrupted stays so.
    pub fn lower_request_hand(self: &Arc<Self>, id: &str) {
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        if self.session_requests.is_waiting_for(id) {
            return;
        }
        let Some(remembered) = hands.remove(id) else {
            return;
        };
        if remembered != SessionStatus::WaitingUser {
            if let Err(err) =
                self.apply_reported_status_if(id, SessionStatus::WaitingUser, remembered)
            {
                tracing::debug!(session = %id, %err, "lowering the session request's hand failed");
                return;
            }
        }
        // Released only now: while it is held, an `agent_status` reader cannot see "no hand" while
        // the record still says `WaitingUser`.
        drop(hands);
        // A message queued while the hand was up is still queued, and no hook event is coming to
        // release it.
        self.spawn_flush_outbox(id, remembered);
    }

    /// Whether [`Self::raise_request_hand`] has something remembered for the session.
    #[cfg(test)]
    pub fn has_request_hand(&self, id: &str) -> bool {
        self.request_hands
            .lock()
            .expect("request hands lock poisoned")
            .contains_key(id)
    }

    /// Drops what [`Self::raise_request_hand`] remembered for a session that is going away.
    pub fn forget_request_hand(&self, id: &str) {
        self.request_hands
            .lock()
            .expect("request hands lock poisoned")
            .remove(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::hooks::BackgroundTasksChange;
    use crate::protocol::SessionStatus;
    use crate::state::test_support::{insert_record, session_record, subagent};
    use crate::state::AppState;
    use crate::test_support::app_state;

    /// A session request holds the record at waiting for the user while its agent stays idle: its
    /// subagents still count, and a working one's do not.
    #[tokio::test]
    async fn a_session_request_does_not_make_an_idle_agent_busy() {
        let (state, dir) = app_state("background-tasks-request-hand");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        state.raise_request_hand("s").unwrap();
        assert!(state.is_idle("s"));
        let started = BackgroundTasksChange::SubagentStarted("a".to_string());
        state.apply_background_tasks_change("s", started);
        assert_eq!(state.background_tasks_of("s"), vec![subagent("a")]);

        state
            .store
            .insert_session(&session_record("w", SessionStatus::Working))
            .unwrap();
        state.raise_request_hand("w").unwrap();
        assert!(!state.is_idle("w"));
    }

    /// A request's hand is not the agent leaving its prompt: a paused session keeps the flag under
    /// it, hidden while the hand is up, and is shown paused again once it is lowered.
    #[tokio::test]
    async fn a_request_hand_keeps_the_paused_flag_of_an_idle_agent() {
        let (state, dir) = app_state("request-hand-paused");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        state.set_paused("s", true);
        let shown = |state: &AppState| state.wire_session(state.session_record("s").unwrap());

        state.raise_request_hand("s").unwrap();
        assert_eq!(shown(&state).status, SessionStatus::WaitingUser);
        assert!(!shown(&state).paused);
        assert!(state.last_turn_abnormal("s"));

        state.lower_request_hand("s");
        assert_eq!(shown(&state).status, SessionStatus::Idle);
        assert!(shown(&state).paused);
    }
}
