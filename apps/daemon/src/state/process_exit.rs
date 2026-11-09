//! The end of a session's process: the stop of every session at shutdown, and the terminal output
//! saved as each one ends; the settling of every session before a planned stop, which ends none;
//! and the stop of every session when an ordinary stop escalates a planned stop already begun.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::AppState;
use crate::session::LiveSession;

/// How long a shutdown waits, for all its sessions together, for the hook events already queued
/// to be applied before it records them interrupted. Far below the seconds the application gives
/// the daemon to stop (`DAEMON_EXIT_GRACE` in the desktop shell's `sidecar/stop.rs`), which the agents'
/// own graceful exit has to fit in too; events are applied in well under a millisecond, so only a
/// wedged queue reaches it.
const SHUTDOWN_HOOK_SETTLE: Duration = Duration::from_millis(500);

/// How long a planned stop waits, once it has raised its handover barrier, for the operations
/// admitted before to finish, the ends of the processes they began included (each waited for
/// until it has ended, which takes the graceful period of a few seconds), and the removal of the
/// branches they leave: archives, removals and restarts end their processes within seconds, and an
/// outbox write is cut short after its current message. Bounded, because the update or restart
/// that asked for the stop waits on it; what is still running then is the next daemon's to adopt
/// as after a crash, and a step that has not begun by then does not begin.
const HANDOVER_BARRIER_WAIT: Duration = Duration::from_secs(5);

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
        self.settle_sessions(&sessions).await;
        self.interrupt_records(&sessions);
        self.attribute_before_ending().await;
        let handles: Vec<_> = sessions
            .into_iter()
            .map(|live| self.spawn_end(live))
            .collect();
        for handle in handles {
            let _ = handle.await;
        }
        self.end_held_launches().await;
    }

    /// [`Self::stop_all_sessions`] for an ordinary stop that escalates a planned stop already
    /// begun. A session whose terminal is held by one of `left`, the holders the planned stop has
    /// handed terminals over from, parted from or left to the next daemon, is told nothing: its
    /// program ends with its holder, which `end_left` ends, never terminal by terminal. Every
    /// session's screen is kept as its preview before anything else, the screen as it is now for
    /// a session still the daemon's to end and the screen up to the handover's mark for one handed
    /// over, so that a stop cut short by whoever is waiting for the daemon to go still leaves the
    /// previews on disk. `end_left` runs once what the agents started has been attributed,
    /// alongside the ends of the other sessions, and the launches of the sessions it ends are
    /// ended in the ledger after it. Every session is left interrupted, as
    /// [`Self::stop_all_sessions`] leaves it.
    pub async fn stop_all_sessions_after_handover(
        self: &Arc<Self>,
        left: &HashSet<String>,
        end_left: impl FnOnce() + Send + 'static,
    ) {
        self.shutting_down.store(true, Ordering::Release);
        let sessions = self.live_sessions();
        let (with_holder, own): (Vec<_>, Vec<_>) = sessions.iter().cloned().partition(|live| {
            live.held_at()
                .is_some_and(|at| left.contains(&at.holder_id))
        });
        // First, and all at once.
        let keeping: Vec<_> = own
            .iter()
            .map(|live| {
                let (state, live) = (self.clone(), live.clone());
                tokio::task::spawn_blocking(move || {
                    // Frozen now, as its end would freeze it: the frozen screen is final, so its
                    // reader is not waited for.
                    live.freeze_output();
                    if let Some(frozen) = live.frozen_output() {
                        state.keep_saved_output(&live.id, &frozen);
                    }
                })
            })
            .chain(with_holder.iter().map(|live| {
                let (state, live) = (self.clone(), live.clone());
                // A handed-over terminal's reader stops at its mark, and is waited for a moment
                // to get there: what it read up to there is the screen.
                tokio::task::spawn_blocking(move || state.save_output(&live))
            }))
            .collect();
        for kept in keeping {
            let _ = kept.await;
        }
        self.settle_sessions(&sessions).await;
        self.interrupt_records(&sessions);
        self.attribute_before_ending().await;
        let ending_left = tokio::task::spawn_blocking(end_left);
        let handles: Vec<_> = own.into_iter().map(|live| self.spawn_end(live)).collect();
        for handle in handles {
            let _ = handle.await;
        }
        let _ = ending_left.await;
        let ending: Vec<_> = with_holder
            .into_iter()
            .map(|live| {
                let ledger = Arc::clone(&self.process_ledger);
                tokio::task::spawn_blocking(move || ledger.end_launch(&live.launch_id))
            })
            .collect();
        for ended in ending {
            let _ = ended.await;
        }
        self.end_held_launches().await;
    }

    /// Marks each of `sessions` interrupted, as a stop leaves it.
    fn interrupt_records(&self, sessions: &[Arc<LiveSession>]) {
        for live in sessions {
            if let Err(err) = self.interrupt_session_record(&live.id) {
                tracing::warn!(session = %live.id, %err, "marking the session interrupted failed");
            }
        }
    }

    /// Closes the process ledger and attributes what the agents started a moment ago, while their
    /// chains through the agents still show, before any agent is ended. Nothing is relaunched from
    /// here on, so nothing is carried over.
    async fn attribute_before_ending(&self) {
        self.process_ledger.close();
        let ledger = Arc::clone(&self.process_ledger);
        let _ = tokio::task::spawn_blocking(move || ledger.scan_before_ending()).await;
    }

    /// Ends the session `live` as a stop ends it, on a blocking thread: `terminate` waits out the
    /// graceful period. The output is saved here rather than left to the exit watcher, which the
    /// daemon may not outlive.
    fn spawn_end(self: &Arc<Self>, live: Arc<LiveSession>) -> tokio::task::JoinHandle<()> {
        let state = self.clone();
        tokio::task::spawn_blocking(move || {
            live.terminate();
            state.save_output(&live);
            // The exit watcher would too, but the daemon may not outlive it.
            state.process_ledger.end_launch(&live.launch_id);
        })
    }

    /// Ends what an account switch under way held for its relaunch, which will not come.
    async fn end_held_launches(&self) {
        let ledger = Arc::clone(&self.process_ledger);
        let _ = tokio::task::spawn_blocking(move || ledger.end_held_launches()).await;
    }

    /// Readies every running session for a planned stop, which hands its terminal over to its
    /// holder and ends nothing. First the handover barrier is raised: nothing more is admitted
    /// that would change or end a terminal or remove what one works in, and what was admitted
    /// before is waited for, for at most [`HANDOVER_BARRIER_WAIT`] (see `super::handover_barrier`),
    /// so that the sessions listed here are the ones it left. Then nothing is started any more,
    /// nothing of this daemon records an exit, and the hook events and program status reports
    /// already received are applied, as [`Self::stop_all_sessions`] applies them, so the next
    /// daemon finds each session's status as they left it. Returns the running sessions.
    pub async fn settle_for_planned_stop(self: &Arc<Self>) -> Vec<Arc<LiveSession>> {
        // Before the flags below: an operation still admitted may be waiting for the exit of a
        // process it ended to be recorded, which nothing records once the handover has begun.
        if !self.raise_handover_barrier(HANDOVER_BARRIER_WAIT).await {
            tracing::warn!("operations admitted before the planned stop were still running when its wait ran out; the terminals they change may be adopted as after a crash");
        }
        self.shutting_down.store(true, Ordering::Release);
        self.handing_over.store(true, Ordering::Release);
        let sessions = self.live_sessions();
        self.settle_sessions(&sessions).await;
        sessions
    }

    /// Applies the hook events queued for `sessions` and the program status reports their readers
    /// took in, for at most [`SHUTDOWN_HOOK_SETTLE`] together.
    async fn settle_sessions(&self, sessions: &[Arc<LiveSession>]) {
        let settling = async {
            for live in sessions {
                self.hook_queues.settled(&live.id).await;
                // The same goes for a program's status report already read.
                live.settle_program_status().await;
            }
        };
        let _ = tokio::time::timeout(shutdown_hook_settle(), settling).await;
    }

    /// Keeps the frozen or final parsed screen after exit. **Blocks** on the
    /// file and on the PTY reader (see `LiveSession::keep_final_output`).
    pub(crate) fn save_output(&self, live: &LiveSession) {
        live.keep_final_output(|output| {
            self.keep_saved_output(&live.id, output);
        });
    }

    /// Keeps `output` as the terminal preview of the session `id`, in place of any other, and says
    /// whether it was written. **Blocks** on the file.
    pub(crate) fn keep_saved_output(&self, id: &str, output: &[u8]) -> bool {
        let written = self.saved_output.write(id, output);
        // A record deleted while its process was exiting may have had its file removed before the
        // write above. Looked at after writing, and the deletion removes the file after the
        // record, so whichever comes second leaves no file behind.
        if let Ok(None) = self.store.get_session(id) {
            self.saved_output.remove(id);
        }
        written
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

    /// An ordinary stop that escalates a planned one keeps each session's preview, leaves it
    /// interrupted, ends a session the daemon still runs itself as an ordinary stop does, and has
    /// the holders left to the next daemon ended.
    #[tokio::test]
    async fn an_escalated_stop_keeps_each_preview_and_ends_what_is_left() {
        let (state, dir) = app_state("state-output-kept-on-escalation");
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

        let ended_left = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ending = ended_left.clone();
        state
            .stop_all_sessions_after_handover(&std::collections::HashSet::new(), move || {
                ending.store(true, std::sync::atomic::Ordering::Release);
            })
            .await;
        let saved = state.saved_output("s").expect("the output was kept");
        assert!(String::from_utf8_lossy(&saved).contains("still here"));
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Interrupted
        );
        assert!(live.poll_exit(), "the session's process was not ended");
        assert!(ended_left.load(std::sync::atomic::Ordering::Acquire));
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
        // A `biased` `join!` polls the stop first, so the queue is drained once the stop has begun
        // and waits on it.
        let drain = async move {
            tokio::task::spawn_blocking(move || {
                crate::hooks::apply::drain_hook_queue(&draining, "s")
            })
            .await
            .unwrap();
        };
        tokio::join!(biased; state.stop_all_sessions(), drain);

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
