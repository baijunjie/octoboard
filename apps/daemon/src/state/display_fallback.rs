//! The colours of the display a client last reported, kept in the store across a restart for
//! `AppState::display_fallback`.

use std::sync::Arc;

use super::AppState;

impl AppState {
    /// Seeds [`Self::display_fallback`] with the colours the store holds, so that a terminal the
    /// daemon starts or adopts before any client has reported answers its program's colour queries
    /// with the last display a client reported, and starts the task that stores each change a
    /// report later makes, one write at a time and off the runtime's workers. Called once, within
    /// the runtime, before anything that starts or adopts a terminal. A value that cannot be read
    /// is left out. The task ends once `DisplayFallback::stop_storing_changes` has closed the
    /// channel and what was queued is written; the stop awaits the returned handle for that.
    pub fn restore_display_fallback(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        match self.store.last_terminal_colors() {
            Ok(Some(text)) => self.display_fallback.seed(&text),
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "the stored terminal colours could not be read"),
        }
        let (changes, mut stored) = tokio::sync::mpsc::unbounded_channel::<String>();
        self.display_fallback.store_changes_through(changes);
        let state = Arc::downgrade(self);
        tokio::spawn(async move {
            while let Some(text) = stored.recv().await {
                let Some(state) = state.upgrade() else { break };
                let written = tokio::task::spawn_blocking(move || {
                    state.store.set_last_terminal_colors(&text)
                })
                .await;
                match written {
                    Ok(Ok(_)) => {}
                    Ok(Err(err)) => {
                        tracing::warn!(%err, "the terminal colours could not be stored");
                    }
                    Err(err) => tracing::warn!(%err, "storing the terminal colours failed"),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::protocol::TerminalAppearance;
    use crate::state::AppState;
    use crate::terminal::display::{DisplayFacts, DisplaySeed};
    use crate::test_support::{app_state, app_state_again, tasks_idle, ScratchDir};

    fn reported_light() -> DisplayFacts {
        DisplayFacts::reported("#383a42", "#fafafa", "#101010", Some((9, 20))).expect("a display")
    }

    fn reported_dark() -> DisplayFacts {
        DisplayFacts::reported("#e4e4e6", "#1e1f22", "#101010", Some((9, 20))).expect("a display")
    }

    /// A client of a terminal reporting `display`.
    fn report(state: &AppState, display: DisplayFacts) {
        DisplaySeed::new(state.display_fallback.clone(), None).record(display);
    }

    /// What a terminal started with no launch appearance answers while no client of it has
    /// reported.
    fn fallback(state: &AppState) -> DisplayFacts {
        DisplaySeed::new(state.display_fallback.clone(), None).fallback()
    }

    /// The store file of an `app_state`, opened by something other than the state, to make its
    /// reads and writes fail.
    fn raw(dir: &ScratchDir) -> Connection {
        Connection::open(dir.join("octoboard.db")).expect("the store file")
    }

    fn assert_colours(display: DisplayFacts, expected: DisplayFacts) {
        assert_eq!(display.foreground, expected.foreground);
        assert_eq!(display.background, expected.background);
        assert_eq!(display.cursor, expected.cursor);
        assert_eq!(display.cell, DisplayFacts::default().cell, "never a cell");
    }

    /// Closing the channel at the stop lets the writer task finish what was queued, so a report
    /// made just before the stop is stored when the handle returns.
    #[tokio::test]
    async fn stopping_waits_for_the_queued_colours_to_be_stored() {
        let (state, _dir) = app_state("display-fallback-stop");
        let writer = state.restore_display_fallback();
        report(&state, reported_light());
        state.display_fallback.stop_storing_changes();
        writer.await.unwrap();
        assert_eq!(
            state.store.last_terminal_colors().unwrap().as_deref(),
            Some(reported_light().colors_text().as_str())
        );
    }

    /// What a client reported is stored, and a daemon started again over the same store answers a
    /// terminal with no launch appearance with it before any client has reported, while a terminal
    /// launched for an appearance still takes that.
    #[tokio::test(start_paused = true)]
    async fn a_reported_display_is_stored_and_a_daemon_started_again_answers_with_it() {
        let (state, mut dir) = app_state("display-fallback-restart");
        state.restore_display_fallback();
        assert_eq!(state.store.last_terminal_colors().unwrap(), None);
        report(&state, reported_light());
        tasks_idle().await;
        assert_eq!(
            state.store.last_terminal_colors().unwrap().as_deref(),
            Some(reported_light().colors_text().as_str())
        );

        let next = app_state_again(&mut dir);
        next.restore_display_fallback();
        assert_colours(fallback(&next), reported_light());
        let launched = DisplaySeed::new(
            next.display_fallback.clone(),
            Some(TerminalAppearance::Dark),
        );
        assert_eq!(
            launched.fallback(),
            DisplayFacts::for_appearance(TerminalAppearance::Dark)
        );
        report(&next, reported_dark());
        assert!(fallback(&next).dark(), "a report of this run");
    }

    #[tokio::test(start_paused = true)]
    async fn with_nothing_stored_a_terminal_with_no_appearance_gets_the_dark_default() {
        let (state, _dir) = app_state("display-fallback-empty");
        state.restore_display_fallback();
        assert_eq!(fallback(&state), DisplayFacts::default());
        assert_eq!(state.store.last_terminal_colors().unwrap(), None);
    }

    /// Only a change of the colours is written: a repeat of the colours held, whatever its cell,
    /// leaves the store as it was, which a value put there meanwhile shows.
    #[tokio::test(start_paused = true)]
    async fn only_a_change_of_the_colours_is_written() {
        let (state, _dir) = app_state("display-fallback-changes");
        state.restore_display_fallback();
        report(&state, reported_light());
        tasks_idle().await;
        state.store.set_last_terminal_colors("marker").unwrap();

        let other_cell =
            DisplayFacts::reported("#383a42", "#fafafa", "#101010", Some((7, 15))).unwrap();
        report(&state, other_cell);
        report(&state, reported_light());
        tasks_idle().await;
        assert_eq!(
            state.store.last_terminal_colors().unwrap().as_deref(),
            Some("marker"),
            "a repeat of the colours is not written"
        );

        report(&state, reported_dark());
        tasks_idle().await;
        assert_eq!(
            state.store.last_terminal_colors().unwrap().as_deref(),
            Some(reported_dark().colors_text().as_str())
        );
    }

    /// The colours the store holds are not written back by a report that repeats them.
    #[tokio::test(start_paused = true)]
    async fn a_report_of_the_stored_colours_is_not_written_again() {
        let (state, _dir) = app_state("display-fallback-stored-repeat");
        state
            .store
            .set_last_terminal_colors(&reported_light().colors_text())
            .unwrap();
        state.restore_display_fallback();
        state.store.set_last_terminal_colors("marker").unwrap();
        report(&state, reported_light());
        tasks_idle().await;
        assert_eq!(
            state.store.last_terminal_colors().unwrap().as_deref(),
            Some("marker")
        );
    }

    /// Text that is no colours is taken as nothing stored: the start goes on with the default, and
    /// the next report replaces the text.
    #[tokio::test(start_paused = true)]
    async fn unreadable_stored_colours_are_ignored_and_replaced_by_a_report() {
        for text in [
            "not json",
            r##"{"foreground":"#383a42","background":"nope","cursor":"#101010"}"##,
        ] {
            let (state, _dir) = app_state("display-fallback-unreadable");
            state.store.set_last_terminal_colors(text).unwrap();
            state.restore_display_fallback();
            assert_eq!(fallback(&state), DisplayFacts::default(), "{text}");

            report(&state, reported_light());
            tasks_idle().await;
            assert_eq!(
                state.store.last_terminal_colors().unwrap().as_deref(),
                Some(reported_light().colors_text().as_str()),
                "{text}"
            );
        }
    }

    /// A store that cannot be read only logs: the start goes on with the default.
    #[tokio::test(start_paused = true)]
    async fn a_store_that_cannot_be_read_does_not_stop_the_start() {
        let (state, dir) = app_state("display-fallback-read-fails");
        raw(&dir)
            .execute("UPDATE settings SET last_terminal_colors = X'00'", [])
            .unwrap();
        assert!(state.store.last_terminal_colors().is_err());
        state.restore_display_fallback();
        assert_eq!(fallback(&state), DisplayFacts::default());

        report(&state, reported_light());
        assert_colours(fallback(&state), reported_light());
    }

    /// A write the store refuses only logs: the colours are still the daemon's last report in
    /// memory, and a later change is stored once the store takes writes again.
    #[tokio::test(start_paused = true)]
    async fn a_write_the_store_refuses_does_not_stop_later_writes() {
        let (state, dir) = app_state("display-fallback-write-fails");
        state.restore_display_fallback();
        let connection = raw(&dir);
        connection
            .execute_batch(
                "CREATE TRIGGER refuse_colours BEFORE UPDATE OF last_terminal_colors ON settings
                 BEGIN SELECT RAISE(ABORT, 'refused'); END;",
            )
            .unwrap();

        report(&state, reported_light());
        tasks_idle().await;
        assert_eq!(state.store.last_terminal_colors().unwrap(), None);
        assert_colours(fallback(&state), reported_light());

        connection
            .execute_batch("DROP TRIGGER refuse_colours;")
            .unwrap();
        report(&state, reported_dark());
        tasks_idle().await;
        assert_eq!(
            state.store.last_terminal_colors().unwrap().as_deref(),
            Some(reported_dark().colors_text().as_str())
        );
    }
}
