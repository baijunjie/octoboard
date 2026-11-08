//! The end of a session's process: the stop of every session at shutdown, and the terminal output
//! saved as each one ends.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::AppState;
use crate::session::LiveSession;

/// How long a shutdown waits, for all its sessions together, for the hook events already queued
/// to be applied before it records them interrupted. Far below the seconds the application gives
/// the daemon to stop (`DAEMON_EXIT_GRACE` in the desktop shell's `sidecar.rs`), which the agents'
/// own graceful exit has to fit in too; events are applied in well under a millisecond, so only a
/// wedged queue reaches it.
const SHUTDOWN_HOOK_SETTLE: Duration = Duration::from_millis(500);

/// [`SHUTDOWN_HOOK_SETTLE`], widened to `PATIENCE` under test so a test holding an event back is
/// not raced by it.
fn shutdown_hook_settle() -> Duration {
    #[cfg(test)]
    {
        SHUTDOWN_HOOK_SETTLE.max(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    SHUTDOWN_HOOK_SETTLE
}

impl AppState {
    /// Ends every running session, leaving each one interrupted and resumable. Used by the exit
    /// flow and by a signal: the daemon owns the agent processes, so none of them may outlive it.
    pub async fn stop_all_sessions(self: &Arc<Self>) {
        self.shutting_down.store(true, Ordering::Release);
        let sessions = self.live_sessions();
        // A turn the agent ended a moment ago may still be on its way through the hook queue.
        // Applied first, its `Stop` decides how that turn ended; applied after the interruption
        // below, it would find the turn recorded as cut short and the session no longer running
        // for the flag to be cleared on.
        let settling = async {
            for live in &sessions {
                self.hook_queues.settled(&live.id).await;
            }
        };
        let _ = tokio::time::timeout(shutdown_hook_settle(), settling).await;
        // Before the interruption below gives them the status of any other interrupted session:
        // the next start resumes these when the user asked for that.
        let ids: Vec<String> = sessions.iter().map(|live| live.id.clone()).collect();
        if let Err(err) = self.store.mark_left_live(&ids) {
            tracing::warn!(%err, "marking the sessions left running failed");
        }
        for live in &sessions {
            if let Err(err) = self.interrupt_session_record(&live.id) {
                tracing::warn!(session = %live.id, %err, "marking the session interrupted failed");
            }
        }
        // What the agents started a moment ago is attributed while their chains through the
        // agents still show. Nothing is relaunched from here on, so nothing is carried over.
        self.process_ledger.close();
        let ledger = Arc::clone(&self.process_ledger);
        let _ = tokio::task::spawn_blocking(move || ledger.scan_before_ending()).await;
        // `terminate` waits out the graceful period, so it goes to a blocking thread. The output is
        // saved here rather than left to each exit watcher, which the daemon may not outlive.
        let handles: Vec<_> = sessions
            .into_iter()
            .map(|live| {
                let state = self.clone();
                tokio::task::spawn_blocking(move || {
                    live.terminate();
                    state.save_output(&live);
                    // The exit watcher would too, but the daemon may not outlive it.
                    state.process_ledger.end_launch(&live.launch_id);
                })
            })
            .collect();
        for handle in handles {
            let _ = handle.await;
        }
        // What an account switch under way held for its relaunch, which will not come.
        let ledger = Arc::clone(&self.process_ledger);
        let _ = tokio::task::spawn_blocking(move || ledger.end_held_launches()).await;
    }

    /// Keeps the frozen or final parsed screen after exit. **Blocks** on the
    /// file and on the PTY reader (see `LiveSession::keep_final_output`).
    pub(crate) fn save_output(&self, live: &LiveSession) {
        live.keep_final_output(|output| {
            self.saved_output.write(&live.id, output);
            // A record deleted while its process was exiting may have had its file removed before
            // the write above. Looked at after writing, and the deletion removes the file after
            // the record, so whichever comes second leaves no file behind.
            if let Ok(None) = self.store.get_session(&live.id) {
                self.saved_output.remove(&live.id);
            }
        });
    }

    /// The terminal preview saved for this session, if one was kept.
    pub fn saved_output(&self, id: &str) -> Option<Vec<u8>> {
        self.saved_output.read(id)
    }

    /// Removes the saved output of every session the store has no record of, and any file left
    /// half-written. Run once at startup, before anything is served.
    pub fn sweep_saved_output(&self) -> Result<()> {
        let ids: std::collections::HashSet<String> = self
            .store
            .list_sessions()?
            .into_iter()
            .map(|session| session.id)
            .collect();
        self.saved_output.sweep(|id| ids.contains(id));
        Ok(())
    }

    /// Drops a session's saved output. Called after its record is deleted, never before: see
    /// [`Self::save_output`]. Also when a new process takes the session over, whose output replaces
    /// it.
    pub fn forget_saved_output(&self, id: &str) {
        self.saved_output.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{error_code, AgentId, CodedError, Event, SessionStatus};
    use crate::session::spawn_reader_thread;
    use crate::state::test_support::insert_record;
    use crate::test_support::{app_state, fake_live_session, PATIENCE};
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn a_no_process_status_publishes_only_after_its_screen_is_frozen() {
        for status in [SessionStatus::Archived, SessionStatus::Interrupted] {
            let (state, dir) = app_state("state-output-frozen-before-publication");
            insert_record(&state, &dir, "s", SessionStatus::Working);
            let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
            state.register_live(live.clone());
            live.on_output(b"\x1b[?1049h\x1b[Hconversation");
            let mut events = state.subscribe();
            let mut record = state.session_record("s").unwrap();
            record.status = status;
            state.save_session(&record).unwrap();
            assert!(matches!(
                events.try_recv(),
                Ok(Event::SessionUpserted { .. })
            ));
            let frozen = live
                .frozen_output()
                .expect("published status has a snapshot");
            live.on_output(b"\x1b[?1049l\x1b[2J\x1b[Hexit message");
            state.save_output(&live);
            assert_eq!(state.saved_output("s"), Some(frozen));
            assert!(live.write_input(b"should stay archived").is_err());
        }
    }

    /// The daemon does not wait for its exit watchers on the way out, so a shutdown keeps the
    /// output itself.
    #[tokio::test]
    async fn stopping_every_session_keeps_each_ones_output() {
        let (state, dir) = app_state("state-output-kept-on-shutdown");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session(
            "s",
            AgentId::CLAUDE,
            80,
            24,
            "printf 'still here'; sleep 30",
        );
        spawn_reader_thread(live.clone(), 8 * 1024);
        state.register_live(live.clone());
        let deadline = Instant::now() + PATIENCE;
        while crate::test_support::printed(&live, 64).is_empty() {
            assert!(Instant::now() < deadline, "the stand-in printed nothing");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        state.stop_all_sessions().await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("still here"));
    }

    /// A `Stop` queued before the shutdown and still being applied when it begins is applied
    /// before the session is recorded interrupted, so the turn it ended normally is not recorded
    /// as cut short.
    #[tokio::test]
    async fn a_stop_still_being_applied_at_shutdown_decides_how_the_turn_ended() {
        let (state, dir) = app_state("state-shutdown-stop-in-flight");
        insert_record(&state, &dir, "s", SessionStatus::Working);
        let live = fake_live_session("s", AgentId::CLAUDE, 80, 24, "sleep 30");
        state.register_live(live.clone());
        let stop = bytes::Bytes::from_static(br#"{"hook_event_name": "Stop"}"#);
        state.hook_queues.push("s", stop);

        let draining = state.clone();
        let drain = async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            tokio::task::spawn_blocking(move || {
                crate::hooks::apply::drain_hook_queue(&draining, "s")
            })
            .await
            .unwrap();
        };
        tokio::join!(state.stop_all_sessions(), drain);

        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(!state.last_turn_abnormal("s"));
    }

    /// Once the daemon has begun to stop its sessions, no claim is granted, and a process that
    /// was already past its claim is ended as soon as it is registered, so a launch under way
    /// cannot start an agent that outlives the stop.
    #[tokio::test]
    async fn no_agent_starts_or_stays_after_the_sessions_were_stopped() {
        let (state, _dir) = app_state("state-stopped-for-good");
        let early = crate::test_support::idle_stand_in("early");
        state.register_live(early.clone());

        state.stop_all_sessions().await;

        for refused in [state.begin_launch("s").err(), state.begin_switch("s").err()] {
            let err = refused.expect("refused");
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(coded.code, error_code::DAEMON_SHUTTING_DOWN);
        }
        let late = crate::test_support::idle_stand_in("late");
        let late_check = late.clone();
        state.register_live(late.clone());
        let ended =
            crate::test_support::within(crate::test_support::PATIENCE, move || late.poll_exit())
                .await;
        assert!(ended, "the late process was ended");
        assert!(
            late_check.was_ended_on_purpose(),
            "by `end`, so its owner is not told"
        );
    }
}
