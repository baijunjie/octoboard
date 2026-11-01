//! The status a session is shown at and the status transitions made through the record, and
//! `wire_session`, which readies a record for the wire.

use anyhow::Result;

use super::AppState;
use crate::protocol::{Session, SessionStatus};
use crate::session::LiveSession;

impl AppState {
    /// The status a client is shown for a session: the stored one, or waiting for the user while
    /// its running process is held up by something only the user can answer that no hook reports,
    /// or starting while a change of its account's sharing has it ended and is to resume it, or
    /// while a change of its model restarts it.
    pub(crate) fn shown_status(&self, session: &Session) -> SessionStatus {
        self.status_when(session, LiveSession::waits_for_user)
    }

    /// [`Self::shown_status`] as an automatic account switch judges it: a session whose only wait
    /// is a usage-limit dialog is not waiting, since ending that dialog is what the switch is for.
    pub(crate) fn status_for_switch(&self, session: &Session) -> SessionStatus {
        self.status_when(session, LiveSession::waits_for_user_beyond_usage_limit)
    }

    /// [`Self::status_for_switch`] as a change of the session's model judges it, which holds the
    /// session's claim and may have it shown starting already: the status the session would be
    /// shown without that change.
    pub(crate) fn status_for_model_change(&self, session: &Session) -> SessionStatus {
        self.status_beside_model_change(session, LiveSession::waits_for_user_beyond_usage_limit)
    }

    fn status_when(&self, session: &Session, waits: fn(&LiveSession) -> bool) -> SessionStatus {
        if session.status != SessionStatus::Archived && self.is_changing_model(&session.id) {
            return SessionStatus::Starting;
        }
        self.status_beside_model_change(session, waits)
    }

    fn status_beside_model_change(
        &self,
        session: &Session,
        waits: fn(&LiveSession) -> bool,
    ) -> SessionStatus {
        let waiting = !session.status.is_dormant()
            && self
                .live_session(&session.id)
                .is_some_and(|live| waits(&live));
        if waiting {
            SessionStatus::WaitingUser
        } else if session.status == SessionStatus::Interrupted && self.is_restarting(&session.id) {
            // Ended by a change of its account's sharing, which resumes it once it has ended.
            SessionStatus::Starting
        } else {
            session.status
        }
    }

    /// The session as a client receives it, with its resolved title and live user interactions.
    /// These leave the stored agent status intact so answering restores it even before a hook.
    pub(crate) fn wire_session(&self, mut session: Session) -> Session {
        session.status = self.shown_status(&session);
        session.background_tasks = self.shown_background_tasks(&session);
        session.switching_account = self.is_switching_account(&session.id);
        session.prompt_on_launch = self.is_prompted_on_launch(&session.id);
        session.default_effort =
            if session.observed_model.is_some() && session.observed_effort.is_none() {
                self.default_effort_of(&session.id)
            } else {
                None
            };
        session.terminal_title = self.resolved_title(&session, &std::cell::OnceCell::new());
        session
    }

    /// Applies a status reported by a hook, unless a waiting session request is holding the
    /// hand up: then the status is only remembered, as what the agent's events say, and the record
    /// keeps `WaitingUser`. Dormant statuses are never reached this way — a session becomes
    /// archived by the user's request and interrupted by its process going away, both of which are
    /// decided here rather than by the agent. Says whether the record's status changed.
    pub fn apply_hook_status(&self, id: &str, status: SessionStatus) -> Result<bool> {
        // Held across the write, so a request opening at the same moment cannot have its hand
        // lowered by a status read before it.
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        let status = if let Some(remembered) = hands.get_mut(id) {
            *remembered = status;
            SessionStatus::WaitingUser
        } else {
            status
        };
        match self.store.set_running_status(id, status)? {
            Some(session) => {
                self.publish_session(&session);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// The status the session's agent is at, which is the record's except while a session
    /// request holds the hand up or a visible action prompt overrides the hooks. Whatever the
    /// daemon decides about the agent itself reads this; clients receive `wire_session`.
    pub fn agent_status(&self, session: &Session) -> SessionStatus {
        if !session.status.is_dormant()
            && self
                .live_session(&session.id)
                .is_some_and(|live| live.needs_user_action())
        {
            return SessionStatus::WaitingUser;
        }
        self.request_hands
            .lock()
            .expect("request hands lock poisoned")
            .get(&session.id)
            .copied()
            .unwrap_or(session.status)
    }

    /// [`Self::apply_reported_status_if`] for a reporter that watches the agent's own status: with
    /// a hand held and the agent remembered at `expected`, the new status replaces what is
    /// remembered and the record keeps the request's `WaitingUser`. Otherwise it is the record that
    /// is `expected` to be moved. Says whether anything moved.
    pub fn report_agent_status_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
    ) -> Result<bool> {
        // Held across the fall-through, for the reason [`Self::apply_hook_status`] holds it.
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        match hands.get_mut(id) {
            Some(remembered) if *remembered == expected => {
                *remembered = status;
                Ok(true)
            }
            Some(_) => Ok(false),
            None => self.apply_reported_status_if(id, expected, status),
        }
    }

    /// Applies a status on behalf of a reporter whose conclusion only holds while the session is
    /// still `expected`, and says whether it moved. The check and the write are one step, so a
    /// conclusion that has gone stale cannot overwrite what moved the session on.
    ///
    /// A hook event needs none of this and cannot use it: it reports an event that happened, which
    /// is true whatever the session's status is, so it has no status to expect.
    pub fn apply_reported_status_if(
        &self,
        id: &str,
        expected: SessionStatus,
        status: SessionStatus,
    ) -> Result<bool> {
        // `expected` is never dormant — the statuses a reporter waits on all mean a process is
        // running — which is what lets the conditional write stand in for the dormancy guard
        // [`Self::apply_hook_status`] needs. `status` must not be dormant either: a session becomes
        // dormant together with its `ended_at`, and this writes the status column alone.
        debug_assert!(!expected.is_dormant());
        debug_assert!(!status.is_dormant());
        match self.store.update_session_status_if(id, expected, status)? {
            Some(session) => {
                self.publish_session(&session);
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{Agent, Event, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session};

    #[test]
    fn codex_manual_approval_overrides_auto_review_and_concurrent_hook_statuses() {
        let (state, dir) = app_state("codex-manual-approval");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let mut record = state.session_record("s").unwrap();
        record.agent = Agent::Codex;
        state.save_session(&record).unwrap();
        let live = fake_live_session("s", Agent::Codex, 80, 24, "sleep 30").with_auto_review();
        assert!(live.resolves_approvals_itself);
        state.register_live(live.clone());
        live.on_output(include_bytes!("../../testdata/codex_working_title.bin"));
        let previous_title = state
            .wire_session(state.session_record("s").unwrap())
            .terminal_title;
        let mut events = state.subscribe();
        let approval = include_bytes!("../../testdata/codex_approval_title.bin");
        for byte in approval {
            live.on_output(std::slice::from_ref(byte));
        }
        assert_eq!(state.agent_status(&record), SessionStatus::WaitingUser);
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("the approval must publish its hand even when the task title stays the same")
        };
        assert_eq!(session.status, SessionStatus::WaitingUser);
        assert_eq!(session.terminal_title, previous_title);
        for status in [SessionStatus::Idle, SessionStatus::Working] {
            state.apply_hook_status("s", status).unwrap();
            let record = state.session_record("s").unwrap();
            assert_eq!(state.agent_status(&record), SessionStatus::WaitingUser);
            assert_eq!(
                state.wire_session(record).status,
                SessionStatus::WaitingUser
            );
        }
        live.on_output(b"\x1b]0;[ . ] Action Required | octoboard\x07");
        assert!(
            live.needs_user_action(),
            "the blinking title must keep the hand up"
        );
        state.raise_request_hand("s").unwrap();
        live.on_output(include_bytes!("../../testdata/codex_working_title.bin"));
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::WaitingUser
        );
        assert_eq!(
            state.agent_status(&state.session_record("s").unwrap()),
            SessionStatus::Working
        );
        state.forget_request_hand("s");
        // Dormancy is written by the coordinator, never by hooks.
        let mut record = state.session_record("s").unwrap();
        record.status = SessionStatus::Interrupted;
        state.save_session(&record).unwrap();
        live.on_output(approval);
        assert_eq!(
            state.wire_session(record.clone()).status,
            SessionStatus::Interrupted
        );
        assert_eq!(state.agent_status(&record), SessionStatus::Interrupted);
    }
}
