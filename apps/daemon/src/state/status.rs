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
    /// while a change of its model restarts it, or dormant while its screen shows a view of its
    /// background agents over a working or idle session.
    pub(crate) fn shown_status(&self, session: &Session) -> SessionStatus {
        self.status_when(session, LiveSession::waits_for_user_outside_agents_view)
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
        let live = self
            .live_session(&session.id)
            .filter(|_| !session.status.has_no_process());
        if live.as_ref().is_some_and(|live| waits(live)) {
            SessionStatus::WaitingUser
        } else if session.status == SessionStatus::Interrupted && self.is_restarting(&session.id) {
            // Ended by a change of its account's sharing, which resumes it once it has ended.
            SessionStatus::Starting
        } else if matches!(session.status, SessionStatus::Working | SessionStatus::Idle)
            && live.is_some_and(|live| live.shows_agents_view())
        {
            SessionStatus::Dormant
        } else {
            session.status
        }
    }

    /// The session as a client receives it, with its resolved title and live user interactions.
    /// These leave the stored agent status intact so answering restores it even before a hook.
    pub(crate) fn wire_session(&self, mut session: Session) -> Session {
        session.status = self.shown_status(&session);
        session.background_tasks = self.shown_background_tasks(&session.id, session.status);
        session.paused = self.shown_paused(&session, session.status);
        session.switching_account = self.is_switching_account(&session.id);
        session.prompt_on_launch = self.is_prompted_on_launch(&session.id);
        session.screen_inexact = self
            .live_session(&session.id)
            .is_some_and(|live| live.screen_inexact());
        session.holder_link = self.shown_holder_link(&session.id);
        session.default_effort =
            if session.observed_model.is_some() && session.observed_effort.is_none() {
                self.default_effort_of(&session.id)
            } else {
                None
            };
        session.terminal_title = self.resolved_title(&session, &std::cell::OnceCell::new());
        session
    }

    /// Whether a client is shown the session as paused: the stored record `session` says its last
    /// turn ended abnormally (see [`Self::set_paused`]) and the status shown, `shown`, is `Idle`.
    /// A session shown at any other status is not paused, whatever its record says.
    pub(crate) fn shown_paused(&self, session: &Session, shown: SessionStatus) -> bool {
        shown == SessionStatus::Idle && session.paused
    }

    /// Whether the stored record of session `id` says its last turn ended abnormally, whatever
    /// its status, for tests to see the flag a client would be shown at `idle`.
    #[cfg(test)]
    pub(crate) fn last_turn_abnormal(&self, id: &str) -> bool {
        self.session_record(id)
            .expect("the session is recorded")
            .paused
    }

    /// Records whether the session's last turn ended abnormally (`true`) or normally (`false`),
    /// and publishes the session when that changes what a client is shown while it is already
    /// idle: a status write that follows does not publish for an unchanged `Idle`. Called before
    /// the status write of a turn's end, so that write carries the flag. A session without a
    /// process has nothing to record: its end settled how its last turn ended.
    pub fn set_paused(&self, id: &str, paused: bool) {
        match self.store.set_last_turn_abnormal(id, paused) {
            Ok(Some(session)) if session.status == SessionStatus::Idle => {
                self.publish_session(&session)
            }
            Ok(_) => {}
            Err(err) => {
                tracing::debug!(session = %id, %err, "recording how the last turn ended failed")
            }
        }
    }

    /// Publishes the stored session when it is idle, which is where a change of what a client is
    /// shown for it (its background tasks) is not carried by a status write.
    pub(super) fn publish_if_idle(&self, id: &str) {
        match self.store.get_session(id) {
            Ok(Some(session)) if session.status == SessionStatus::Idle => {
                self.publish_session(&session)
            }
            Ok(_) => {}
            Err(err) => {
                tracing::debug!(session = %id, %err, "reading the session to publish it failed")
            }
        }
    }

    /// Applies a status reported by a hook, unless a waiting session request is holding the
    /// hand up: then the status is only remembered, as what the agent's events say, and the record
    /// keeps `WaitingUser`. No-process statuses are never reached this way — a session becomes
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
            // The record keeps the request's `WaitingUser`, so the write below moves nothing: the
            // agent moving off its prompt clears the flag here instead, unpublished, since the
            // session is not shown paused while the hand is up.
            if status != SessionStatus::Idle {
                self.store.set_last_turn_abnormal(id, false)?;
            }
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
        if !session.status.has_no_process()
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

    /// [`Self::apply_reported_status_if`] to `Idle` for a reporter that watches the agent's own
    /// status and has found its turn ended abnormally (cancelled, declined, a failure) outside the
    /// hooks: with a hand held and the agent remembered at `expected`, `Idle` replaces what is
    /// remembered and the record keeps the request's `WaitingUser`. Otherwise it is the record that
    /// is `expected` to be moved. Says whether anything moved. The session is marked paused only
    /// when it moved, in the same write, so the record published carries the flag and a session
    /// that had moved on is left as its own events say.
    pub fn report_paused_if(&self, id: &str, expected: SessionStatus) -> Result<bool> {
        debug_assert!(!expected.has_no_process());
        // Held across the fall-through, for the reason [`Self::apply_hook_status`] holds it.
        let mut hands = self
            .request_hands
            .lock()
            .expect("request hands lock poisoned");
        match hands.get_mut(id) {
            Some(remembered) if *remembered == expected => {
                self.store.set_last_turn_abnormal(id, true)?;
                *remembered = SessionStatus::Idle;
                Ok(true)
            }
            Some(_) => Ok(false),
            None => Ok(self.published_if_moved(self.store.pause_session_if(id, expected)?)),
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
        // `expected` is never a no-process status — the statuses a reporter waits on all mean a
        // process is running — which is what lets the conditional write stand in for the
        // no-process guard [`Self::apply_hook_status`] needs. `status` must not be a no-process
        // status either: a session loses its process together with its `ended_at`, and this
        // writes the status column alone. A status off the prompt clears the paused flag in the
        // same write.
        debug_assert!(!expected.has_no_process());
        debug_assert!(!status.has_no_process());
        Ok(self.published_if_moved(self.store.update_session_status_if(id, expected, status)?))
    }

    /// Publishes the record a conditional status write returned, and says whether it moved.
    fn published_if_moved(&self, moved: Option<Session>) -> bool {
        match moved {
            Some(session) => {
                self.publish_session(&session);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AgentId, Event, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session, SUBAGENT_VIEW};

    #[test]
    fn codex_manual_approval_overrides_auto_review_and_concurrent_hook_statuses() {
        let (state, dir) = app_state("codex-manual-approval");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let mut record = state.session_record("s").unwrap();
        record.agent = AgentId::CODEX;
        state.save_session(&record).unwrap();
        let live = fake_live_session("s", AgentId::CODEX, 80, 24, "sleep 30").with_auto_review();
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
        // The no-process status is written by the coordinator, never by hooks.
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

    // Claude Code's other two views of its background agents, as drawn (see `session::dialogs`),
    // and a dialog that is a plain wait for the user.
    const AGENTS_VIEW: &str = "New sessions: Opus 5.5 \u{b7} medium effort\n\
        0 awaiting input \u{b7} 1 working \u{b7} 0 completed\n\
        \u{276F} describe a task for a new session\n  enter to return \u{b7} space to reply\n";
    const AGENT_LIST_FOCUS: &str = "\u{276F} \n  Enter to view \u{b7} x to stop\n  \u{23FA} main\n\
        \u{276F} \u{25EF} general-purpose  Slow probe agent\n";
    const RESUME_IDLE: &str = "  Resume this conversation?\n\
        \x20 This conversation has been inactive for 1h and is 42k tokens long.\n\
        \x20 \u{276F} Resume\n    Start a new conversation\n\n\
        \x20 Enter to confirm \u{b7} Esc to dismiss\n";

    /// A stored session at `stored` whose live process shows `screen`, as the shown status judges.
    fn showing(
        label: &str,
        stored: SessionStatus,
        screen: &str,
    ) -> (
        std::sync::Arc<AppState>,
        crate::test_support::ScratchDir,
        Session,
    ) {
        let (state, dir) = app_state(label);
        insert_record(&state, &dir, "s", stored);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        live.on_output(screen.as_bytes());
        assert!(live.look_for_dialog().current.is_some());
        let record = state.session_record("s").unwrap();
        (state, dir, record)
    }

    /// Each view of the background agents shows a working or idle session as dormant, in what a
    /// client receives and in what an automatic switch and a model change judge; the stored status
    /// stays what the agent reported.
    #[test]
    fn a_view_of_the_background_agents_shows_a_working_or_idle_session_as_dormant() {
        for screen in [SUBAGENT_VIEW, AGENTS_VIEW, AGENT_LIST_FOCUS] {
            for stored in [SessionStatus::Working, SessionStatus::Idle] {
                let (state, _dir, record) = showing("status-dormant", stored, screen);
                assert_eq!(state.shown_status(&record), SessionStatus::Dormant);
                assert_eq!(state.status_for_switch(&record), SessionStatus::Dormant);
                assert_eq!(
                    state.status_for_model_change(&record),
                    SessionStatus::Dormant
                );
                assert_eq!(state.wire_session(record).status, SessionStatus::Dormant);
                assert_eq!(state.session_record("s").unwrap().status, stored);
            }
        }
    }

    /// A hand a hook raised still wins over a view, and every other dialog is still a wait for the
    /// user, as is a view's session without a process or at starting.
    #[test]
    fn a_view_does_not_hide_a_wait_for_the_user() {
        let (state, _dir, record) = showing(
            "status-dormant-waiting",
            SessionStatus::WaitingUser,
            SUBAGENT_VIEW,
        );
        assert_eq!(state.shown_status(&record), SessionStatus::WaitingUser);
        assert_eq!(state.status_for_switch(&record), SessionStatus::WaitingUser);

        for stored in [SessionStatus::Working, SessionStatus::Idle] {
            let (state, _dir, record) = showing("status-other-dialog", stored, RESUME_IDLE);
            assert_eq!(state.shown_status(&record), SessionStatus::WaitingUser);
        }

        let (state, _dir, mut record) =
            showing("status-view-ended", SessionStatus::Working, SUBAGENT_VIEW);
        record.status = SessionStatus::Interrupted;
        assert_eq!(state.shown_status(&record), SessionStatus::Interrupted);
        record.status = SessionStatus::Starting;
        assert_eq!(state.shown_status(&record), SessionStatus::Starting);
    }

    /// The flag is shown only while the status shown is idle, and `set_paused` publishes it when
    /// the session is idle already, where the status write that follows would publish nothing.
    #[test]
    fn paused_is_shown_only_while_idle_and_set_paused_publishes_for_an_idle_session() {
        let (state, dir) = app_state("status-paused");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let mut events = state.subscribe();
        let shown = |state: &AppState| state.wire_session(state.session_record("s").unwrap());
        assert!(!shown(&state).paused);

        state.set_paused("s", true);
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("an idle session being paused must be published")
        };
        assert!(session.paused);
        assert!(shown(&state).paused);
        state.set_paused("s", true);
        assert!(
            events.try_recv().is_err(),
            "an unchanged flag publishes nothing"
        );

        // Any status but idle hides it and drops it, with the write that moves the session.
        state
            .apply_hook_status("s", SessionStatus::Working)
            .unwrap();
        assert!(!state.last_turn_abnormal("s"));
        assert!(!shown(&state).paused);
        state.set_paused("s", true);
        assert!(
            !shown(&state).paused,
            "a working session is not shown paused"
        );
        state.apply_hook_status("s", SessionStatus::Idle).unwrap();
        assert!(shown(&state).paused);

        // A session that is not recorded has nothing to be paused.
        state.set_paused("nobody", true);
    }

    /// The flag is the stored record's, so it outlives the process: a session whose process ended
    /// mid-turn comes back paused once relaunched at its prompt, one whose process ended at its
    /// prompt comes back as its last turn left it, and the next turn clears it.
    #[test]
    fn a_turn_cut_short_by_the_process_ending_leaves_the_session_paused_when_it_comes_back() {
        for (at_end, paused_before, paused_after) in [
            (SessionStatus::Working, false, true),
            (SessionStatus::WaitingUser, false, true),
            (SessionStatus::Idle, false, false),
            (SessionStatus::Idle, true, true),
        ] {
            let label = format!("{at_end:?} {paused_before}");
            let (state, dir) = app_state("status-paused-cut-short");
            insert_record(&state, &dir, "s", SessionStatus::Idle);
            state.set_paused("s", paused_before);
            state.apply_hook_status("s", at_end).unwrap();
            let shown = |state: &AppState| state.wire_session(state.session_record("s").unwrap());

            state.store.record_session_exit("s", 1, None).unwrap();
            let ended = shown(&state);
            assert_eq!(ended.status, SessionStatus::Interrupted, "{label}");
            assert!(!ended.paused, "an interrupted session is not shown paused");
            state
                .store
                .update_session_lifecycle_if(
                    "s",
                    SessionStatus::Interrupted,
                    SessionStatus::Idle,
                    None,
                )
                .unwrap()
                .expect("relaunched");
            assert_eq!(shown(&state).paused, paused_after, "{label}");

            state
                .apply_hook_status("s", SessionStatus::Working)
                .unwrap();
            state.apply_hook_status("s", SessionStatus::Idle).unwrap();
            assert!(!shown(&state).paused, "the next turn clears it: {label}");
        }
    }

    /// A view over an idle session hides the flag with the status.
    #[test]
    fn a_view_over_a_paused_session_shows_it_dormant_and_not_paused() {
        let (state, _dir, record) =
            showing("status-paused-view", SessionStatus::Idle, SUBAGENT_VIEW);
        state.set_paused("s", true);
        let shown = state.wire_session(record);
        assert_eq!(shown.status, SessionStatus::Dormant);
        assert!(!shown.paused);
    }

    /// The flag follows a conditional write only when it moved the session: a pause is not marked
    /// on a session that had moved on, and a stale write to another status does not clear it.
    #[test]
    fn the_paused_flag_changes_only_with_a_conditional_write_that_moved_the_session() {
        let (state, dir) = app_state("status-paused-conditional");
        insert_record(&state, &dir, "s", SessionStatus::Working);

        // Expected `WaitingUser`, found `Working`: nothing moved, nothing marked.
        assert!(!state
            .report_paused_if("s", SessionStatus::WaitingUser)
            .unwrap());
        assert!(!state.last_turn_abnormal("s"));

        let mut events = state.subscribe();
        assert!(state.report_paused_if("s", SessionStatus::Working).unwrap());
        assert!(state.last_turn_abnormal("s"));
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("the move must be published")
        };
        assert!(session.paused, "the published record carries the flag");

        // A write expecting a status the session is not at leaves the flag.
        assert!(!state
            .apply_reported_status_if("s", SessionStatus::Working, SessionStatus::WaitingUser)
            .unwrap());
        assert!(state.last_turn_abnormal("s"));
        assert!(state
            .apply_reported_status_if("s", SessionStatus::Idle, SessionStatus::Working)
            .unwrap());
        assert!(!state.last_turn_abnormal("s"));
    }
}
