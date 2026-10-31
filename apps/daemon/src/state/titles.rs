//! A session's title: recording the one its terminal sets, and the one shown.

use super::AppState;
use crate::protocol::Session;
use crate::session::LiveSession;

impl AppState {
    /// The live lock keeps registration/relaunch from replacing the process between checking its
    /// identity and storing its title. Only this column is written: status and user names may move
    /// concurrently. A deleted record is never recreated. An agent re-sends its title as often as
    /// its spinner turns, so the title is always stored but published only when it changes what
    /// clients are shown. Returns whether a visible action prompt has just been answered, so
    /// the observer can release queued messages without writing on the PTY reader thread.
    pub(crate) fn record_terminal_title(&self, session: &LiveSession, title: Option<&str>) -> bool {
        let live = self.live.read().expect("live sessions lock poisoned");
        if !live
            .sessions
            .get(&session.id)
            .is_some_and(|current| std::ptr::eq(current.as_ref(), session))
        {
            return false;
        }
        let names = std::cell::OnceCell::new();
        let before = match self.store.get_session(&session.id) {
            Ok(Some(record)) => self.resolved_title(&record, &names),
            Ok(None) => return false,
            Err(err) => {
                tracing::warn!(session = %session.id, %err, "reading the session for its terminal title failed");
                return false;
            }
        };
        let action_changed = session.observe_action_title(title);
        let stored = self.store.set_terminal_title(&session.id, title);
        drop(live);
        match stored {
            Ok(Some(record))
                if action_changed || self.resolved_title(&record, &names) != before =>
            {
                self.publish_session(&record)
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(session = %session.id, %err, "storing the terminal title failed")
            }
        }
        action_changed && !session.needs_user_action()
    }

    /// The title a client is shown for the session. `names` holds the project's names once asked.
    pub(super) fn resolved_title(
        &self,
        session: &Session,
        names: &std::cell::OnceCell<Vec<String>>,
    ) -> Option<String> {
        crate::session_title::resolve(
            session.agent,
            session.terminal_title.as_deref(),
            &session.record_titles,
            || {
                names
                    .get_or_init(|| {
                        let path = match &session.project_id {
                            Some(project) => self
                                .store
                                .get_project(project)
                                .ok()
                                .flatten()
                                .map(|p| p.path),
                            None => self
                                .store
                                .get_console(&session.console_id)
                                .ok()
                                .flatten()
                                .map(|console| console.workdir),
                        };
                        path.map(|path| {
                            crate::session_title::project_names(std::path::Path::new(&path))
                        })
                        .unwrap_or_default()
                    })
                    .clone()
            },
        )
    }

    /// The name the session list shows for the session: its task title and created name joined by
    /// a fullwidth bar, or the created name alone when there is no task title or it repeats the
    /// name. Clients compose the same from the record they receive; see "Displayed names" in
    /// docs/product/session-titles.md.
    pub(crate) fn display_title(&self, session: &Session) -> String {
        match self.resolved_title(session, &std::cell::OnceCell::new()) {
            Some(task) if !task.trim().is_empty() && task.trim() != session.title.trim() => {
                format!("{}｜{}", task.trim(), session.title)
            }
            _ => session.title.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{Agent, Event, SessionStatus};
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session};

    #[test]
    fn terminal_titles_reach_unattached_sessions_without_stale_processes_or_records_winning() {
        let (state, dir) = app_state("state-terminal-title");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let original = state.session_record("s").unwrap();
        let first = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        let mut events = state.subscribe();
        let output = b"before\x1b]2;First task\x07after";
        first.on_output(output);
        assert!(
            events.try_recv().is_err(),
            "no record update before registration"
        );
        state.register_live(first.clone());
        assert_eq!(first.recent_output(1024), output, "raw output is unchanged");
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected a title update")
        };
        assert_eq!(session.terminal_title.as_deref(), Some("First task"));
        first.on_output(b"\x1b]2;First task\x07");
        assert!(
            events.try_recv().is_err(),
            "equal titles are not announced again"
        );
        state.save_session(&original).unwrap();
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected the saved session")
        };
        assert_eq!(
            session.terminal_title.as_deref(),
            Some("First task"),
            "a stale caller cannot blank the title"
        );
        let successor = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(successor.clone());
        first.on_output(b"\x1b]2;Stale task\x07");
        assert!(events.try_recv().is_err());
        assert_eq!(
            state.session_record("s").unwrap().terminal_title.as_deref(),
            Some("First task")
        );
        successor.on_output(b"\x1b]2;Next task\x1b\\");
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected successor title")
        };
        assert_eq!(session.terminal_title.as_deref(), Some("Next task"));
        let mut archived = state.session_record("s").unwrap();
        archived.status = SessionStatus::Archived;
        state.save_session(&archived).unwrap();
        events.try_recv().unwrap();
        for cleanup in [b"\x1b]2;\x07".as_slice(), b"\x1b]2;octoboard\x07"] {
            successor.on_output(cleanup);
            assert!(
                events.try_recv().is_err(),
                "shutdown titles are not published"
            );
            assert_eq!(
                state.session_record("s").unwrap().terminal_title.as_deref(),
                Some("Next task"),
                "archiving preserves the active title through process cleanup"
            );
        }
        assert!(state.delete_if_archived("s").unwrap());
        assert!(matches!(
            events.try_recv().unwrap(),
            Event::SessionDeleted { .. }
        ));
        successor.on_output(b"\x1b]2;Deleted task\x07");
        assert!(events.try_recv().is_err());
        assert!(state.store.get_session("s").unwrap().is_none());
    }

    #[test]
    fn decorations_that_leave_the_resolved_title_alone_are_not_published() {
        let (state, dir) = app_state("state-title-decoration");
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let live = fake_live_session("s", Agent::Claude, 80, 24, "sleep 30");
        state.register_live(live.clone());
        let mut events = state.subscribe();
        live.on_output(b"\x1b]2;\xe2\x9c\xb3 Claude Code\x07");
        assert!(events.try_recv().is_err(), "the placeholder names nothing");
        live.on_output("\x1b]2;◐ Task\x07".as_bytes());
        let Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected a title update")
        };
        assert_eq!(session.terminal_title.as_deref(), Some("Task"));
        live.on_output("\x1b]2;✳ Task\x07".as_bytes());
        assert!(events.try_recv().is_err(), "a new glyph is not a new title");
        assert_eq!(
            state.session_record("s").unwrap().terminal_title.as_deref(),
            Some("Task"),
            "the title is stored without its leading glyph"
        );
    }
}
